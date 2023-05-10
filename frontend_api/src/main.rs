use std::{collections::HashMap, sync::Mutex, env};

use forntend_api_lib::{FrontendApi, websocket, PeerMap};
use messages::messages::{NewDisplayEvent, DisplayEvent, AIEvent};

#[tokio::main]
async fn main() {
    let service = FrontendApi{
        host: "127.0.0.1".to_string(),
        port: 9000,
    };

    let (sender,receiver) = std::sync::mpsc::channel();

    println!("Starting api server");
    let addr = env::args().nth(1).unwrap_or_else(|| "127.0.0.1:9000".to_string());

    let state = PeerMap::new(Mutex::new(HashMap::new()));
    println!("got here");
    let state2 = state.clone();
    tokio::spawn(async move { websocket(addr, state2).await});

    tokio::spawn(async move { service.run(receiver, state.clone())});

    loop {
        sender.send(NewDisplayEvent{event: DisplayEvent::ChannelFollow(AIEvent{story_segment: "Hello".to_string(), icon_uri: "none".to_string()})}).unwrap();

        // put thread to sleep for 10 seconds
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}



