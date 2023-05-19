use std::{collections::HashMap, env, sync::Mutex, time::Duration};

use forntend_api_lib::{accept_connection, ConnectionMap};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    env_logger::init();

    let addr = "127.0.0.1:9000";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    println!("Listening on: {}", addr);

    let state = ConnectionMap::new(Mutex::new(HashMap::new()));

    let state2 = state.clone();

    tokio::spawn(async move {
        let mut count = 0;
        loop {
            {
                let mut state2 = state2.lock().unwrap();

                for (&addr, tx) in state2.iter_mut() {
                    println!("Sending message to: {}", addr);
                    if tx
                        .unbounded_send(Message::Text(format!("{}", count)))
                        .is_err()
                    {
                        println!("closing websocket message to: {} ==========", addr);
                    }
                }
                count += 1;
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });

    while let Ok((stream, _)) = listener.accept().await {
        let peer = stream
            .peer_addr()
            .expect("connected streams should have a peer address");
        println!("Peer address: {}", peer);

        tokio::spawn(accept_connection(peer, stream, state.clone()));
    }
}
