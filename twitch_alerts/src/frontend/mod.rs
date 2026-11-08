use crate::messages::DisplayMessage;
use anyhow::Context;
use axum::{routing::get, Router};
use futures_channel::mpsc::unbounded;
use futures_util::{SinkExt, StreamExt};
use maud::html;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinSet,
};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Message, Result},
};
use tower_http::services::ServeDir;

mod htmx;
mod network;
mod routes;
mod types;

pub use network::{FrontendApi, HostInfo};
use routes::{admin, index};
use types::{ConnectionMap, EventQueues, Queues};

#[derive(Clone)]
pub struct FrontendState {
    pub host_info: HostInfo,
    pub event_queues: EventQueues,
}

impl FrontendApi {
    pub fn new(host_info: HostInfo, asset_path: String) -> Self {
        Self {
            host_info,
            connection_state: Arc::new(Mutex::new(HashMap::new())),
            asset_path,
        }
    }

    pub async fn run(
        &self,
        receiver: mpsc::UnboundedReceiver<DisplayMessage>,
    ) -> anyhow::Result<()> {
        // Bind both listeners before starting background work.
        let websocket_listener = TcpListener::bind(self.host_info.get_ws_address())
            .await
            .context("bind frontend WebSocket listener")?;
        let http_listener = TcpListener::bind(self.host_info.get_http_address())
            .await
            .context("bind frontend HTTP listener")?;
        tracing::info!(http = %http_listener.local_addr()?, websocket = %websocket_listener.local_addr()?, "Frontend listening");

        let event_queues = Arc::new(Mutex::new(Queues::new()));
        let app = frontend_router(
            FrontendState {
                host_info: self.host_info.clone(),
                event_queues: event_queues.clone(),
            },
            self.asset_path.clone(),
        );

        tokio::try_join!(
            async {
                axum::serve(http_listener, app)
                    .await
                    .context("serve frontend HTTP")
            },
            receive_messages(receiver, &event_queues),
            process_events(&event_queues, &self.connection_state),
            accept_connections(websocket_listener, self.connection_state.clone()),
        )?;
        Ok(())
    }
}

async fn receive_messages(
    mut receiver: mpsc::UnboundedReceiver<DisplayMessage>,
    queues: &EventQueues,
) -> anyhow::Result<()> {
    while let Some(message) = receiver.recv().await {
        queue_message(queues, message);
    }
    anyhow::bail!("alert message channel closed")
}

fn queue_message(queues: &EventQueues, message: DisplayMessage) {
    let mut queues = queues.lock().expect("event queue lock poisoned");
    queues.unpublished_events.push_back(message.clone());
    queues.latest_events.push_back(message);
    if queues.latest_events.len() > 10 {
        queues.latest_events.pop_front();
    }
}

fn next_message(queues: &EventQueues) -> Option<DisplayMessage> {
    if !types::EVENT_QUEUE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    queues
        .lock()
        .expect("event queue lock poisoned")
        .unpublished_events
        .pop_front()
}

fn broadcast(connections: &ConnectionMap, message: Message) {
    connections
        .lock()
        .expect("connection lock poisoned")
        .retain(|peer, sender| {
            let connected = sender.unbounded_send(message.clone()).is_ok();
            if !connected {
                tracing::debug!(%peer, "Removing disconnected browser");
            }
            connected
        });
}

async fn process_events(queues: &EventQueues, connections: &ConnectionMap) -> anyhow::Result<()> {
    loop {
        let Some(message) = next_message(queues) else {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            continue;
        };
        let html = html! {
            div id="notifications" class="alert" hx-swap="afterend" hx-target="notifications" {
                div class="wrapper" {
                    (htmx::get_display_html(&message))
                }
            }
        };
        broadcast(connections, Message::Text(html.into()));
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        let html = html! {
            div id="notifications" hx-swap="delete" hx-target="notifications" {}
        };
        broadcast(connections, Message::Text(html.into()));
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

async fn accept_connections(listener: TcpListener, state: ConnectionMap) -> anyhow::Result<()> {
    // Dropping the set cancels active browser connections when the server stops.
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            connection = listener.accept() => {
                let (stream, peer) = connection.context("accept browser WebSocket")?;
                connections.spawn(accept_connection(peer, stream, state.clone()));
            }
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                result.context("browser connection task failed")?;
            }
        }
    }
}

async fn accept_connection(peer: SocketAddr, stream: TcpStream, state: ConnectionMap) {
    let result = handle_connection(peer, stream, &state).await;
    state
        .lock()
        .expect("connection lock poisoned")
        .remove(&peer);
    if let Err(error) = result {
        match error {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
            error => tracing::warn!(%peer, %error, "Browser WebSocket failed"),
        }
    }
}

async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    state: &ConnectionMap,
) -> Result<()> {
    let socket = accept_async(stream).await?;
    let (sender, mut receiver) = unbounded();
    state
        .lock()
        .expect("connection lock poisoned")
        .insert(peer, sender);
    let (mut socket_sender, mut socket_receiver) = socket.split();
    loop {
        tokio::select! {
            message = socket_receiver.next() => {
                let Some(message) = message else { break };
                let message = message?;
                if message.is_text() || message.is_binary() {
                    socket_sender.send(message).await?;
                } else if message.is_close() {
                    break;
                }
            }
            message = receiver.next() => {
                let Some(message) = message else { break };
                socket_sender.send(message).await?;
            }
        }
    }
    Ok(())
}

fn frontend_router(state: FrontendState, asset_path: String) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/admin", get(admin))
        .route("/events/latest", get(routes::get_latest_unpublished_events))
        .route("/tts", get(routes::get_all_events_in_queue))
        .route("/events", get(routes::get_all_events_in_queue))
        .route("/events/latest/all", get(routes::get_latest_events))
        .route("/events/pause", get(routes::pause_events))
        .route("/events/start", get(routes::resume_events))
        .nest_service("/assets", ServeDir::new(asset_path))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_alert_pages_and_assets() {
        serves_alert_pages_and_assets_result()
            .await
            .expect("test failed");
    }

    async fn serves_alert_pages_and_assets_result() -> anyhow::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let state = FrontendState {
            host_info: HostInfo {
                http_host: "127.0.0.1".into(),
                http_port: address.port(),
                websocket_host: "localhost".into(),
                ws_port: 9000,
            },
            event_queues: Arc::new(Mutex::new(Queues::new())),
        };
        let router = frontend_router(
            state,
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/frontend/assets").into(),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        let client = reqwest::Client::builder().no_proxy().build()?;
        for page in ["/", "/admin"] {
            let body = client
                .get(format!("http://{address}{page}"))
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            assert!(body.contains("ws-connect=\"ws://localhost:9000/\""));
            assert!(body.contains("/assets/websocket.js"));
        }
        for asset in ["/assets/css/alert.css", "/assets/websocket.js"] {
            let body = client
                .get(format!("http://{address}{asset}"))
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            assert!(!body.is_empty());
        }
        server.abort();
        Ok(())
    }

    #[tokio::test]
    async fn closed_alert_channel_returns_an_error() {
        let (sender, receiver) = mpsc::unbounded_channel();
        drop(sender);
        let queues = Arc::new(Mutex::new(Queues::new()));
        let result = receive_messages(receiver, &queues).await;
        assert!(result.unwrap_err().to_string().contains("channel closed"));
    }

    #[test]
    fn broadcast_removes_disconnected_clients() {
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let (live_sender, mut live_receiver) = unbounded();
        let (closed_sender, closed_receiver) = unbounded();
        drop(closed_receiver);
        let live_peer = "127.0.0.1:10001".parse().unwrap();
        let closed_peer = "127.0.0.1:10002".parse().unwrap();
        connections.lock().unwrap().insert(live_peer, live_sender);
        connections
            .lock()
            .unwrap()
            .insert(closed_peer, closed_sender);
        broadcast(&connections, Message::Text("alert".into()));
        assert_eq!(connections.lock().unwrap().len(), 1);
        assert!(connections.lock().unwrap().contains_key(&live_peer));
        assert_eq!(
            live_receiver.try_recv().unwrap(),
            Message::Text("alert".into())
        );
    }

    #[tokio::test]
    async fn occupied_http_port_returns_an_error() {
        let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let frontend = FrontendApi::new(
            HostInfo {
                http_host: "127.0.0.1".into(),
                http_port: occupied.local_addr().unwrap().port(),
                websocket_host: "localhost".into(),
                ws_port: 0,
            },
            String::new(),
        );
        let (_sender, receiver) = mpsc::unbounded_channel();
        let result = frontend.run(receiver).await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("bind frontend HTTP listener"));
    }

    #[tokio::test]
    async fn queued_follow_alert_reaches_browser_websocket() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let queues = Arc::new(Mutex::new(Queues::new()));
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut workers = JoinSet::new();
        let incoming = queues.clone();
        workers.spawn(async move { receive_messages(receiver, &incoming).await });
        let browsers = connections.clone();
        workers.spawn(async move { accept_connections(listener, browsers).await });
        workers.spawn(async move { process_events(&queues, &connections).await });
        let (mut browser, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        sender
            .send(DisplayMessage {
                message: "A local adventurer joins the party.".into(),
                image_url: "none".into(),
                sound_url: "none".into(),
                display_time: 1000,
                payload: crate::messages::TwitchEvent::ChannelFollow(
                    crate::messages::FollowEvent {
                        user_name: "LocalFollower".into(),
                        user_id: 123,
                    },
                ),
            })
            .unwrap();
        let alert = tokio::time::timeout(std::time::Duration::from_secs(3), browser.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let html = alert.into_text().unwrap();
        assert!(html.contains("id=\"notifications\""));
        assert!(html.contains("class=\"alert\""));
        assert!(html.contains("Followed"));
        assert!(html.contains("LocalFollower"));
        assert!(html.contains("A local adventurer joins the party."));
        browser.close(None).await.unwrap();
    }

    #[tokio::test]
    async fn websocket_disconnect_removes_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(HashMap::new()));
        let server_state = state.clone();
        let server = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            accept_connection(peer, stream, server_state).await;
        });
        let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        socket.send(Message::Text("hello".into())).await.unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap(),
            Message::Text("hello".into())
        );
        assert_eq!(state.lock().unwrap().len(), 1);
        socket.close(None).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
        assert!(state.lock().unwrap().is_empty());
    }
}
