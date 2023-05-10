use std::{sync::{mpsc::Receiver, Arc, Mutex}, env, collections::HashMap};
use messages::messages::NewDisplayEvent;
use tokio::{net::{TcpListener, TcpStream}};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt, SinkExt};
use std::net::SocketAddr;

use tokio_tungstenite::tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
pub type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;
pub struct FrontendApi {
    pub host: String,
    pub port: u16,
}

impl FrontendApi  {
    pub async fn new(host: String, port: u16) -> Self {
        Self {
            host: host,
            port: port,
        }
    }

    pub fn run(&self, receiver: Receiver<NewDisplayEvent>, state: PeerMap) -> Result<(), eyre::Error> {

        loop {
            match receiver.recv() {
                Ok(event) => {
                    println!("Received event: {:?}", event);
                    send_message_to_all(state.clone(), Message::text("Ping"));
                }
                Err(e) => {
                    println!("Error receiving event: {:?}", e);
                    println!("{}", e);
                    break;
                }
            }
        }
        Ok(())
    }
}

fn send_message_to_all(state: PeerMap, msg: Message) {
    let mut state = state.lock().unwrap();
    let mut closed = Vec::new();

    for (&addr, tx) in state.iter_mut() {
        if tx.unbounded_send(msg.clone()).is_err() {
            closed.push(addr);
        }
    }

    for addr in closed {
        state.remove(&addr);
    }
}

async fn handle_connection(state: PeerMap, stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let mut ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx,mut rx) = unbounded();
    state.lock().unwrap().insert(addr, tx);

    loop {
        match rx.next().await {
            Some(_) => {
                ws_stream.send(Message::text("Ping")).await.unwrap()
            },
            None => todo!(),
        }
    }
}

pub async fn websocket(addr: String, state: PeerMap) {

    println!("Inside websocket function");
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);
    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        println!("Peer address: {}", addr);
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }
} 