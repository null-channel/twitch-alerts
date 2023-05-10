
#[derive(Debug)]
pub struct NewDisplayEvent {
    pub event: DisplayEvent,
}

#[derive(Debug)]
pub enum DisplayEvent {
    ChannelFollow(AIEvent),
    ChannelSubscribe(AIEvent),
}

#[derive(Debug)]
pub struct AIEvent {
    pub story_segment: String,
    pub icon_uri: String,
}

