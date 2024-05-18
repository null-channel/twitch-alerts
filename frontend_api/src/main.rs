use forntend_api_lib::FrontendApi;
use tokio::sync::mpsc;

//TODO: This should run like the "full app" does in the lib.rs file
#[tokio::main]
async fn main() {
    env_logger::init();

    // this main function should be the same as the one in the lib.rs file
    // and is expected to be used for local testing
    let ws_address = "0.0.0.0:9000";
    let http_address = "0.0.0.0:8080";

    let api = FrontendApi::new(ws_address.to_string(), http_address.to_string());
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        api.run(rx).await.unwrap();
    });

    //TODO: write some test something to send a message to the receiver

    loop {
        let display_message = messages::DisplayMessage {
            message: "hello from htmx".to_string(),
            image_url: "".to_string(),
            sound_url: "".to_string(),
            display_time: 5000,
            payload: messages::TwitchEvent::ChannelFollow(messages::FollowEvent {
                user_name: "some user".to_string(),
                user_id: 123,
            }),
        };

        let _ = tx.send(display_message).unwrap();

        tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    }
}
