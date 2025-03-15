use forntend_api_lib::{FrontendApi, HostInfo};

use tokio::sync::mpsc;

//TODO: This should run like the "full app" does in the lib.rs file

#[tokio::main]
async fn main() {
    env_logger::init();

    // this main function should be the same as the one in the lib.rs file
    // and is expected to be used for local testing
    let ws_port = "9000";
    let ws_hostname = "0.0.0.0";
    let http_address = "8080";

    let host_info = HostInfo {
        websocket_host: ws_hostname.to_string(),
        ws_port: ws_port.parse().unwrap(),
        http_port: http_address.parse().unwrap(),
    };
    let api = FrontendApi::new(host_info, "assets".to_string());

    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        api.run(rx).await.unwrap();
    });

    //TODO: write some test something to send a message to the receiver

    let mut count = 0;
    loop {
        count += 1;
        let display_message = messages::DisplayMessage {
            message: format!("hello from htmx {}", count),
            image_url: "".to_string(),
            sound_url: "".to_string(),
            display_time: 10000,
            payload: messages::TwitchEvent::ChannelFollow(messages::FollowEvent {
                user_name: "some user".to_string(),
                user_id: 123,
            }),
        };

        tx.send(display_message).unwrap();

        tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    }
}
