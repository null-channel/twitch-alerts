use serde::{Deserialize, Serialize};
pub use twitch_common::messages::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DisplayMessage {
    pub message: String,
    pub image_url: String,
    pub sound_url: String,
    pub display_time: usize,
    pub payload: TwitchEvent,
}
