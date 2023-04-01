use async_trait::async_trait;
use common::TwitchEventConsumer;
use twitch_api::eventsub::Event;

fn main() {
    println!("Hello, world!");
}

pub struct AIManager<EventProducer> {
    pub producer: EventProducer,
    pub sqlite_pool: sqlx::SqlitePool,
}

#[async_trait]
impl TwitchEventConsumer for AIManager<EventProducer> {
    async fn new_event(
        &self,
        event: Event,
        message_id: String,
        message_at: String,
    ) -> anyhow::Result<()> {
        let mut conn = self.sqlite_pool.acquire().await?;

        Ok(())
    }
}
