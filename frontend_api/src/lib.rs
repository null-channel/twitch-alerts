use std::{sync::{mpsc::Receiver, Arc, Mutex}, env, collections::HashMap};
use messages::messages::NewDisplayEvent;
use tokio::{net::{TcpListener, TcpStream}};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use std::net::SocketAddr;
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt, SinkExt};

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
                Ok(_event) => {
                    send_message_to_all_connected_cleints(state.clone(), Message::text("Ping"));
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

fn send_message_to_all_connected_cleints(state: PeerMap, msg: Message) {
    let mut state = state.lock().unwrap();
    let mut closed = Vec::new();

    for (&addr, tx) in state.iter_mut() {
        println!("Sending message to: {}", addr);
        if tx.unbounded_send(msg.clone()).is_err() {
            closed.push(addr);
        }
    }

    /*
    for addr in closed {
        println!("closing websocket message to: {} ==========", addr);
        state.remove(&addr);
    }
    */
}

async fn handle_connection(state: PeerMap, stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx,rx) = unbounded();
    {
        state.lock().unwrap().insert(addr, tx);
    }

    println!("Finished locking state for new stream: {}", addr);
    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        println!("Received a message from {}: {}", addr, msg.to_text().unwrap());
        future::ok(())
    });
    
    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    state.lock().unwrap().remove(&addr);
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