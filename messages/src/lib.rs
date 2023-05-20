use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct NewTwitchEventMessage {
    pub event: TwitchEvent,
    pub message_id: String,
    pub message_at: String,
}

#[derive(Debug)]
pub enum TwitchEvent {
    ChannelFollow(FollowEvent),
}

#[derive(Debug)]
pub struct FollowEvent {
    pub user_name: String,
    pub user_id: i64,
}

// Path: messages/src/lib.rs
// { "{ "message": "hello world", "image_url": "https:://something.com" }" }
#[derive(Serialize, Deserialize, Debug)]
pub struct DisplayMessage {
    pub message: String,
    pub image_url: String,
    pub sound_url: String,
    pub display_time: usize,
}
