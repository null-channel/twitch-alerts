use axum::{routing::get, Router};
use eyre::eyre;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, SinkExt, StreamExt};
use maud::{html, Markup};
use messages::DisplayMessage;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Duration;
use std::{
    collections::HashMap,
    env,
    sync::{mpsc::Receiver, Arc, Mutex},
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

type Tx = UnboundedSender<Message>;
pub type ConnectionMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;
type MessageQueue = Arc<Mutex<VecDeque<DisplayMessage>>>;

pub struct FrontendApi {
    ws_address: String,
    http_address: String,
    connection_state: ConnectionMap,
}

impl FrontendApi {
    pub fn new(ws_address: String, http_address: String) -> FrontendApi {
        FrontendApi {
            ws_address,
            http_address,
            connection_state: ConnectionMap::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(
        &self,
        mut receiver: mpsc::UnboundedReceiver<DisplayMessage>,
    ) -> Result<(), eyre::Error> {
        let listener = TcpListener::bind(&self.ws_address)
            .await
            .expect("Can't listen");
        println!("Listening on: {}", self.ws_address);

        let connection_state = self.connection_state.clone();
        let message_queue_arc: MessageQueue = Arc::new(Mutex::new(VecDeque::new()));

        tokio::spawn(async move {
            loop {
                let msg = (&mut receiver).recv().await;
                handle_message(connection_state.clone(), message_queue_arc.clone(), msg).await;
            }
        });

        let https_address = self.http_address.clone();
        tokio::spawn(async move {
            loop {
                let listener = TcpListener::bind(&https_address)
                    .await
                    .expect("Can't listen");
                // build our application
                let app = Router::new()
                    .route("/", get(index))
                    //TODO: understand where to put our assets
                    // Remember that these need served by nginx in production
                    .nest_service("/assets", ServeDir::new("assets"));

                // run it
                axum::serve(listener, app).await.unwrap();
            }
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

async fn index() -> Markup {
    html! {
        h1 { "Hello, World!" }
    }
}

async fn handle_message(
    connection_state: ConnectionMap,
    message_queue: MessageQueue,
    message: Option<DisplayMessage>,
) {
    match message {
        Some(message) => {
            let mut state2 = connection_state.lock().unwrap();
            for (&addr, tx) in state2.iter_mut() {
                println!("Sending message to: {}", addr);

                //Enqueue message
                {
                    let mut message_queue = message_queue.lock().unwrap();
                    message_queue.push_back(message.clone());
                }

                //TODO: Delete this as we have a message queue
                let Ok(message) = serde_json::to_string(&message) else {
                    println!("Error serializing message");
                    continue;
                };
                if tx.unbounded_send(Message::Text(message.clone())).is_err() {
                    println!("closing websocket message to: {} ==========", addr);
                }
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
    let ws_stream = accept_async(stream).await.expect("Failed to accept");

    let (tx, mut rx) = unbounded();
    {
        state.lock().unwrap().insert(peer, tx);
    }
    println!("New WebSocket connection: {}", peer);
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
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
                            break;
                        }
                    }
                    None => break,
                }
            }

            //TODO need to manage queue here?
            msg = rx.next() => {
                ws_sender.send(msg.unwrap()).await?;
            }
        }
    }

    Ok(())
}
