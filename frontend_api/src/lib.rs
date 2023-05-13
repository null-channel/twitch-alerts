use std::{sync::{mpsc::Receiver, Arc, Mutex}, env, collections::HashMap};
use messages::messages::NewDisplayEvent;
use tokio::{net::{TcpListener, TcpStream}};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use std::net::SocketAddr;
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt, SinkExt};

use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result, Message},
};

type Tx = UnboundedSender<Message>;
pub type ConnectionMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

pub async fn accept_connection(peer: SocketAddr, stream: TcpStream, state: ConnectionMap) {
    if let Err(e) = handle_connection(peer, stream, state).await {
        match e {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
            err => println!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection(peer: SocketAddr, stream: TcpStream, state: ConnectionMap) -> Result<()> {
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