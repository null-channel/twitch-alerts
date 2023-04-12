use std::sync::mpsc::Receiver;

use async_trait::async_trait;
use chatgpt::prelude::{ChatGPT, Conversation};
use common::{NewTwitchEventMessage, TwitchEvent};
use tokio::runtime::Handle;
use twitch_api::eventsub::{channel::ChannelFollowV2Payload, Event, Message, Payload};

pub struct AIManager {
    pub sqlite_pool: sqlx::SqlitePool,
    pub chat_gpt: ChatGPT,
    pub res: Receiver<NewTwitchEventMessage>,
}

impl AIManager {
    
    pub fn new(sqlite: sqlx::SqlitePool, chat_key: String, res: Receiver<NewTwitchEventMessage>) -> anyhow::Result<Self> {
        let chat = ChatGPT::new(chat_key)?;
        Ok(AIManager {
            sqlite_pool: sqlite,
            chat_gpt: chat,
            res: res,
        })
    }

    pub fn start(&self) {
        match self.res.recv() {
            Ok(message) => {
                let res = self.new_event(message);
                match res {
                    Ok(()) => println!("ok"),
                    Err(e) => println!("{}",e),
                }
            },
            Err(e) => return
        }
    }

    fn new_event(
        &self,
        msg: NewTwitchEventMessage
    ) -> anyhow::Result<()> {

        let thread_handle = Handle::current();
        let mut conn = thread_handle.block_on(self.sqlite_pool.acquire())?;

        let mut conversation: Conversation = self.chat_gpt.new_conversation_directed(
            "You are NullGPT, when answering any questions, you always answer with a short epic story involving the Rust programming language and null."
        );

/* 
        match &msg.event {
            TwitchEvent::ChannelFollow(follow_event) => {
                let response = thread_handle.block_on(conversation
                    .send_message(format!(
                        "tell me an epic short story about my new follower {}?",
                        follow_event.user_name
                    )))?;

                println!("Response: {}", response.message().content);
            }
            _ => println!("Not handled event: {:?}", msg.event),
        }
*/
        Ok(())
    }
}

