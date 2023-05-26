pub mod sqlite;

use std::sync::mpsc::Receiver;

use chatgpt::prelude::{ChatGPT, Conversation};
use eyre::eyre;
use messages::{DisplayMessage, NewTwitchEventMessage, TwitchEvent};
use tokio::{runtime::Handle, sync::mpsc};

pub struct AIManager {
    pub sqlite_pool: sqlx::SqlitePool,
    pub chat_gpt: ChatGPT,
    pub frontend_sender: mpsc::UnboundedSender<DisplayMessage>,
}

impl AIManager {
    pub fn new(
        sqlite: sqlx::SqlitePool,
        chat_key: String,
        fs: mpsc::UnboundedSender<DisplayMessage>,
    ) -> anyhow::Result<Self> {
        let chat = ChatGPT::new(chat_key)?;
        Ok(AIManager {
            sqlite_pool: sqlite,
            chat_gpt: chat,
            frontend_sender: fs,
        })
    }

    pub async fn run(
        &self,
        mut receiver: mpsc::UnboundedReceiver<NewTwitchEventMessage>,
    ) -> Result<(), eyre::Error> {
        loop {
            let msg = (&mut receiver).recv().await;

            match msg {
                Some(message) => {
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
                None => return Err(eyre!("error: receiver closed")),
            }
        }
    }

    async fn new_event(&self, msg: NewTwitchEventMessage) -> anyhow::Result<()> {
        let mut conversation: Conversation = self.chat_gpt.new_conversation_directed(
            "You are D&DGPT, when answering any questions, you always answer with a short epic story as a dungeons and dragons dungeon master in 65 words or less."
        );

        match &msg.event {
            TwitchEvent::ChannelFollow(follow_event) => {
                let response = conversation
                    .send_message(format!(
                        "tell me an epic story about how {} became my new follower?",
                        follow_event.user_name
                    ))
                    .await?;

                println!("Response: {}", response.message().content);
                let mut conn = self.sqlite_pool.acquire().await?;
                let db_results = sqlite::write_new_story_segment(
                    conn,
                    follow_event.user_id,
                    "follow".to_string(),
                    response.message().content.to_string(),
                )
                .await?;
                println!("db_results: {:?}", db_results);

                let display_time = response.message().content.split(" ").count() * 500;
                //TODO: check if there is a "MAX_DISPLAY_TIME" env var

                let display_message = DisplayMessage {
                    message: response.message().content.to_string(),
                    image_url: "none".to_string(),
                    sound_url: "none".to_string(),
                    display_time: display_time,
                };
                self.frontend_sender.send(display_message)?;
            }
        }
        Ok(())
    }
}
