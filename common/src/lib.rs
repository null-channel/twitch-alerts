use async_trait::async_trait;
use twitch_api::eventsub::Event;
#[async_trait]
pub trait TwitchEventConsumer {
    async fn new_event(
        &self,
        event: Event,
        message_id: String,
        message_at: String,
    ) -> anyhow::Result<()>;
}
