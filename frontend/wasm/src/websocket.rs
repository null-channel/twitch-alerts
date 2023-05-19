use serde::{Deserialize, Serialize};
use serde_json::Result;



struct WebSocketService {
    port: i32,
    url: String,
}


#[derive(Serialize, Deserialize)]
pub struct WebsocketMessage{
    pub message: String,
    pub length: i32,
    pub soundUrl: String,
    pub imageUrl: String,
}