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
}
