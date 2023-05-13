use futures_channel::mpsc::{UnboundedSender, UnboundedReceiver, unbounded};
use futures_util::{SinkExt, StreamExt};
use std::{net::SocketAddr, sync::{Arc, Mutex}, collections::HashMap, time::Duration};
use tokio::{net::{TcpListener, TcpStream}};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result, Message},
};
type Tx = UnboundedSender<Message>;
pub type ConnectionMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

async fn accept_connection(peer: SocketAddr, stream: TcpStream, state: ConnectionMap) {
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
            _ = rx.next() => {
                ws_sender.send(Message::Text("ping".to_owned())).await?;
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let addr = "127.0.0.1:9000";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    println!("Listening on: {}", addr);

    let state = ConnectionMap::new(Mutex::new(HashMap::new()));

    let state2 = state.clone();

    tokio::spawn(async move {
        loop {
            {
                let mut state2 = state2.lock().unwrap();

                for (&addr, tx) in state2.iter_mut() {
                    println!("Sending message to: {}", addr);
                    if tx.unbounded_send(Message::Text("ping".to_owned())).is_err() {
                        println!("closing websocket message to: {} ==========", addr);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(3000)).await;
        }
    });

    while let Ok((stream, _)) = listener.accept().await {
        let peer = stream.peer_addr().expect("connected streams should have a peer address");
        println!("Peer address: {}", peer);

        tokio::spawn(accept_connection(peer, stream, state.clone()));
    }
}