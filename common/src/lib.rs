use std::sync::Arc;

use async_trait::async_trait;
use twitch_api::eventsub::Event;
#[async_trait]
pub trait TwitchEventHandler {
    async fn new_event(
        &self,
        event: Event,
        message_id: String,
        message_at: String,
    ) -> anyhow::Result<()>;
}

pub trait TwitchEventProducer {
    fn register(event_handler: Arc<dyn TwitchEventHandler>);
}
