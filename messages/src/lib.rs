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
    ChannelSubscribe(SubscribeEvent),
    ChannelRaid(RaidEvent),
}

#[derive(Debug)]
pub struct FollowEvent {
    pub user_name: String,
    pub user_id: i64,
}

#[derive(Debug)]
pub struct RaidEvent {
    pub from_broadcaster_user_id: String,
    pub from_broadcaster_user_login: String,
    pub from_broadcaster_user_name: String,
    pub to_broadcaster_user_id: String,
    pub to_broadcaster_user_login: String,
    pub to_broadcaster_user_name: String,
    pub viewers: i64,
}

#[derive(Debug)]
pub struct SubscribeEvent {
    pub broadcaster_user_id: i64,
    pub broadcaster_user_name: String,
    pub user_name: String,
    pub user_id: i64,
    pub is_gift: bool,
    pub tier: SubscriptionTier,
}

#[derive(Debug)]
pub enum SubscriptionTier {
    /// Tier 1. $4.99
    #[cfg_attr(feature = "serde", serde(rename = "1000"))]
    Tier1,
    /// Tier 1. $9.99
    #[cfg_attr(feature = "serde", serde(rename = "2000"))]
    Tier2,
    /// Tier 1. $24.99
    #[cfg_attr(feature = "serde", serde(rename = "3000"))]
    Tier3,
    /// Prime subscription
    Prime,
    /// Other
    Other(String),
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
