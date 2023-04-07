use async_trait::async_trait;
use chatgpt::prelude::{ChatGPT, Conversation};
use common::TwitchEventHandler;
use twitch_api::eventsub::{channel::ChannelFollowV2Payload, Event, Message, Payload};

pub struct AIManager {
    pub sqlite_pool: sqlx::SqlitePool,
    pub chat_gpt: ChatGPT,
}

impl AIManager {
    pub fn new(sqlite: sqlx::SqlitePool, chat_key: String) -> anyhow::Result<Self> {
        let chat = ChatGPT::new(chat_key)?;
        Ok(AIManager {
            sqlite_pool: sqlite,
            chat_gpt: chat,
        })
    }
}

#[async_trait]
impl TwitchEventHandler for AIManager {
    async fn new_event(
        &self,
        event: Event,
        message_id: String,
        message_at: String,
    ) -> anyhow::Result<()> {
        let mut conn = self.sqlite_pool.acquire().await?;

        let mut conversation: Conversation = self.chat_gpt.new_conversation_directed(
            "You are NullGPT, when answering any questions, you always answer with a short epic story involving the Rust programming language and null."
        );

        match &event {
            Event::ChannelFollowV2(Payload {
                message:
                    Message::Notification(ChannelFollowV2Payload {
                        user_name, user_id, ..
                    }),
                ..
            }) => {
                let response = conversation
                    .send_message(format!(
                        "tell me an epic short story about my new follower {}?",
                        user_name
                    ))
                    .await?;

                println!("Response: {}", response.message().content);
            }
            _ => println!("Not handled event: {:?}", event),
        }

        Ok(())
    }
}
