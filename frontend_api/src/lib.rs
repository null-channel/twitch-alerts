use axum::{routing::get, Router};
use futures_channel::mpsc::unbounded;
use futures_util::sink::With;
use futures_util::{SinkExt, StreamExt};
use maud::html;
use messages::DisplayMessage;
use std::net::SocketAddr;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tower_http::services::ServeDir;

use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Message, Result},
};

mod htmx;
mod routes;
mod types;
use routes::{admin, index};

use crate::types::{ConnectionMap, EventQueues, Queues};

pub struct FrontendApi {
    pub host_info: HostInfo,
    pub connection_state: ConnectionMap,
    pub asset_path: String,
}

#[derive(Clone)]
pub struct HostInfo {
    pub websocket_host: String,
    pub ws_port: u16,
    pub http_port: u16,
}

impl HostInfo {
    pub fn get_http_address(&self) -> String {
        format!("{}:{}", "0.0.0.0", self.http_port)
    }

    pub fn get_ws_address(&self) -> String {
        format!("{}:{}", "0.0.0.0", self.ws_port)
    }

    pub fn get_frontend_ws_address(&self) -> String {
        format!("{}:{}", self.websocket_host, self.ws_port)
    }
}

#[derive(Clone)]
pub struct UnitedStates {
    pub host_info: HostInfo,
    pub event_queues: EventQueues,
}

impl FrontendApi {
    pub fn new(host_info: HostInfo, asset_path: String) -> FrontendApi {
        FrontendApi {
            host_info,
            connection_state: ConnectionMap::new(Mutex::new(HashMap::new())),
            asset_path,
        }
    }

    pub async fn run(
        &self,
        mut receiver: mpsc::UnboundedReceiver<DisplayMessage>,
    ) -> Result<(), eyre::Error> {
        let listener = TcpListener::bind(&self.host_info.get_ws_address())
            .await
            .expect("Can't listen");
        println!("Listening on: {}", self.host_info.get_ws_address());

        let connection_state = self.connection_state.clone();
        let message_queue_arc: EventQueues = Arc::new(Mutex::new(Queues::new()));

        //TODO: Need to fetch un presented messages from database

        let queue = message_queue_arc.clone();
        let state = connection_state.clone();
        // Listen for incoming events and store them in the queues
        tokio::spawn(async move {
            loop {
                let msg = (&mut receiver).recv().await;
                handle_message(state.clone(), queue.clone(), msg).await;
            }
        });

        // Process the Queues on a new thread
        let queue_connection_state = connection_state.clone();
        let event_queue = message_queue_arc.clone();
        tokio::spawn(async move {
            loop {
                let active = types::EVENT_QUEUE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst);
                if !active {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    continue;
                }

                let message = {
                    let mut queues = event_queue.lock().unwrap();
                    queues.unpublished_events.pop_front()
                };

                let Some(message) = message else {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    continue;
                };

                //Make html message to send to frontend
                //<div id="alerts" hx-swap-oob="true">
                let html_message = html! {
                    div id="notifications" class="alert" hx-swap="afterend" hx-target="notifications" {
                        div class="wrapper" {
                            (htmx::get_display_html(message.clone()))
                        }
                    }
                };

                let mut bad_websockets = vec![];

                //Send message to all connected websockets
                {
                    let mut websocket_state = queue_connection_state.lock().unwrap();
                    for (&addr, tx) in websocket_state.iter_mut() {
                        if tx
                            .unbounded_send(Message::Text(html_message.clone().into()))
                            .is_err()
                        {
                            println!("closing websocket message to: {} ==========", addr);
                            bad_websockets.push(addr);
                        }
                    }
                }

                //Pause for a bit to allow the message to be displayed
                tokio::time::sleep(tokio::time::Duration::from_millis(10000)).await;

                let html_message = html! {
                    div id="notifications" hx-swap="delete" hx-target="notifications" {
                    }
                };
                {
                    let mut websocket_state = queue_connection_state.lock().unwrap();
                    for (&addr, tx) in websocket_state.iter_mut() {
                        if tx
                            .unbounded_send(Message::Text(html_message.clone().into()))
                            .is_err()
                        {
                            println!("closing websocket message to: {} ==========", addr);
                            bad_websockets.push(addr);
                        }
                    }
                }

                //Pause a bit before running queue again
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });

        let https_address = self.host_info.get_http_address();

        let united_states = UnitedStates {
            host_info: self.host_info.clone(),
            event_queues: message_queue_arc.clone(),
        };

        print!("Frontend HTTP is Listening on: {}", https_address);
        let asset_path = self.asset_path.clone();
        tokio::spawn(async move {
            let listener = TcpListener::bind(&https_address)
                .await
                .expect("Can't listen");
            // build our application
            let app = Router::new()
                .route("/", get(index))
                .route("/admin", get(admin))
                .route("/events/latest", get(routes::get_latest_unpublished_events))
                .route("/tts", get(routes::get_all_events_in_queue))
                .route("/events", get(routes::get_all_events_in_queue))
                .route("/events/latest/all", get(routes::get_latest_events))
                .route("/events/pause", get(routes::pause_events))
                .route("/events/start", get(routes::resume_events))
                //TODO: understand where to put our assets
                // Remember that these need served by nginx in production
                .nest_service("/assets", ServeDir::new(asset_path.clone()))
                .with_state(united_states.clone());

            // run it
            axum::serve(listener, app).await.unwrap();
        });

        let new_connection_state = self.connection_state.clone();

        while let Ok((stream, _)) = listener.accept().await {
            let peer = stream
                .peer_addr()
                .expect("connected streams should have a peer address");
            println!("Peer address: {}", peer);

            tokio::spawn(accept_connection(
                peer,
                stream,
                new_connection_state.clone(),
            ));
        }
        Ok(())
    }
}

async fn handle_message(
    connection_state: ConnectionMap,
    event_queues: EventQueues,
    message: Option<DisplayMessage>,
) {
    match message {
        Some(message) => {
            let mut queues = event_queues.lock().unwrap();

            //TODO: Store different types of messages in different queues
            queues.unpublished_events.push_back(message.clone());

            //add to latest events, remove oldest if over 10
            queues.latest_events.push_back(message.clone());
            if queues.latest_events.len() > 10 {
                queues.latest_events.pop_front();
            }
        }
        None => panic!("Error receiving message"),
    }
}
pub async fn accept_connection(peer: SocketAddr, stream: TcpStream, state: ConnectionMap) {
    if let Err(e) = handle_connection(peer, stream, state).await {
        match e {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
            err => println!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    state: ConnectionMap,
) -> Result<()> {
    println!("New WebSocket connection: {}", peer);
    let ws_stream = accept_async(stream).await.expect("Failed to accept");

    let (tx, mut rx) = unbounded();
    {
        state.lock().unwrap().insert(peer, tx);
    }
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    println!("Connection state: {:?}", state.lock().unwrap().keys());
    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(msg) => {
                        let msg = msg?;
                        // TODO: handle message
                        if msg.is_text() ||msg.is_binary() {
                            println!("Received a message from {}: {}", peer, msg.to_text()?);
                            ws_sender.send(msg).await?;
                        } else if msg.is_close() {
                            println!("Issue with connection: {}", peer);
                            break;
                        }
                    }
                    None => break,
                }
            }

            //TODO need to manage queue here?
            msg = rx.next() => {
                let msg = msg.unwrap();
                println!("Sending message to {}: {}", peer, msg.to_text()?);
                let res = ws_sender.send(msg).await;
                if res.is_err() {
                    println!("Error sending message to {}", peer);
                    break;
                }
            }
        }
    }

    Ok(())
}
