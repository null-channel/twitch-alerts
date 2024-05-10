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
    let (_tx, rx) = mpsc::unbounded_channel();
    api.run(rx).await.unwrap();
    //TODO: write some test something to send a message to the receiver
}
