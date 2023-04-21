pub mod sqlite;

use std::sync::mpsc::Receiver;

use chatgpt::prelude::{ChatGPT, Conversation};
use common::{NewTwitchEventMessage, TwitchEvent};
use eyre::eyre;
use tokio::runtime::Handle;

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

    pub async fn run(&self, receiver: Receiver<NewTwitchEventMessage>) -> Result<(), eyre::Error> {
        loop {
            let msg = receiver.recv();

            match msg {
                Ok(message) => {
                    let res = self.new_event(message).await;
                    match res {
                        Ok(()) => {
                            println!("ok");
                        }
                        Err(e) => {
                            println!("{}", e);
                        }
                    }
                }
                Err(e) => return Err(eyre!("error: {}", e)),
            }
        }
    }

    async fn new_event(&self, msg: NewTwitchEventMessage) -> anyhow::Result<()> {

        let mut conversation: Conversation = self.chat_gpt.new_conversation_directed(
            "You are NullGPT, when answering any questions, you always answer with a short epic story involving the Rust programming language and null in 75 words or less."
        );

        match &msg.event {
            TwitchEvent::ChannelFollow(follow_event) => {
                let response = conversation
                    .send_message(format!(
                        "tell me an epic story about my new follower {}?",
                        follow_event.user_name
                    ))
                    .await?;


                println!("Response: {}", response.message().content);
                let mut conn = self.sqlite_pool.acquire().await?;
                let db_results = sqlite::write_new_story_segment(conn, follow_event.user_id, "follow".to_string(), response.message().content.to_string()).await?;
                println!("db_results: {:?}", db_results);
            }
        }
        Ok(())
    }
}
