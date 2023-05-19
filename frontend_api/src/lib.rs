use eyre::eyre;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, SinkExt, StreamExt};
use messages::DisplayMessage;
use std::net::SocketAddr;
use std::{
    collections::HashMap,
    env,
    sync::{mpsc::Receiver, Arc, Mutex},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Message, Result},
};

type Tx = UnboundedSender<Message>;
pub type ConnectionMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

pub struct FrontendApi {
    address: String,
    connection_state: ConnectionMap,
}

impl FrontendApi {
    pub fn new(addr: String) -> FrontendApi {
        FrontendApi {
            address: addr,
            connection_state: ConnectionMap::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(
        &self,
        mut receiver: mpsc::UnboundedReceiver<DisplayMessage>,
    ) -> Result<(), eyre::Error> {
        let listener = TcpListener::bind(&self.address)
            .await
            .expect("Can't listen");
        println!("Listening on: {}", self.address);

        let state2 = self.connection_state.clone();

        tokio::spawn(async move {
            loop {
                let msg = (&mut receiver).recv().await;

                match msg {
                    Some(message) => {
                        let mut state2 = state2.lock().unwrap();
                        let Ok(message) = serde_json::to_string(&message) else {
                            println!("Error serializing message");
                            continue;
                        };
                        for (&addr, tx) in state2.iter_mut() {
                            println!("Sending message to: {}", addr);

                            if tx.unbounded_send(Message::Text(message.clone())).is_err() {
                                println!("closing websocket message to: {} ==========", addr);
                            }
                        }
                    }
                    None => panic!("Error receiving message"),
                }
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
            msg = rx.next() => {
                ws_sender.send(msg.unwrap()).await?;
            }
        }
    }

    Ok(())
}
