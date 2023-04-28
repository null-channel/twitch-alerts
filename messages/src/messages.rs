
#[derive(Debug)]
struct NewDisplayEvent {
    pub event: DisplayEvent,
}

#[derive(Debug)]
pub enum DisplayEvent {
    ChannelFollow(FollowEvent),
    ChannelSubscribe(SubscribeEvent),
}

#[derive(Debug)]
pub struct FollowEvent {
    pub story_segment: String,
    pub icon_uri: String,
}

#[derive(Debug)]
pub struct SubscribeEvent {
    pub story_segment: String,
    pub icon_uri: String,
}
