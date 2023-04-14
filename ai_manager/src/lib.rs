use std::sync::mpsc::Receiver;

use chatgpt::prelude::{ChatGPT, Conversation};
use common::{NewTwitchEventMessage, TwitchEvent};
use eyre::eyre;
use tokio::runtime::Handle;

pub struct AIManager {
    pub sqlite_pool: sqlx::SqlitePool,
    pub chat_gpt: ChatGPT,
    pub res: Receiver<NewTwitchEventMessage>,
}

impl AIManager {
    pub fn new(
        sqlite: sqlx::SqlitePool,
        chat_key: String,
        res: Receiver<NewTwitchEventMessage>,
    ) -> anyhow::Result<Self> {
        let chat = ChatGPT::new(chat_key)?;
        Ok(AIManager {
            sqlite_pool: sqlite,
            chat_gpt: chat,
            res: res,
        })
    }

    pub fn run(&self) -> Result<(), eyre::Error> {
        let thread_handle = Handle::current();
        loop {
            match self.res.recv() {
                Ok(message) => {
                    let res = thread_handle.block_on(self.new_event(message));
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
        let mut conn = self.sqlite_pool.acquire().await?;

        let mut conversation: Conversation = self.chat_gpt.new_conversation_directed(
            "You are NullGPT, when answering any questions, you always answer with a short epic story involving the Rust programming language and null."
        );

        match &msg.event {
            TwitchEvent::ChannelFollow(follow_event) => {
                let response = conversation
                    .send_message(format!(
                        "tell me an epic short story about my new follower {}?",
                        follow_event.user_name
                    ))
                    .await?;

                println!("Response: {}", response.message().content);
            }
        }
        Ok(())
    }
}
