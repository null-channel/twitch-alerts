pub mod sqlite;

use std::sync::mpsc::Receiver;

use chatgpt::prelude::{ChatGPT, Conversation};
use eyre::eyre;
use messages::{
    ChannelGiftMessage, DisplayMessage, FollowEvent, NewTwitchEventMessage, NullSubTier, RaidEvent,
    SubscribeEvent, TwitchEvent,
};
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
        let conversation: Conversation = self.chat_gpt.new_conversation_directed(
            "You are D&DGPT, when answering any questions, you always answer with a short epic story as a dungeons and dragons dungeon master in 27 words or less."
        );

        match &msg.event {
            TwitchEvent::ChannelFollow(follow_event) => {
                println!("Channel Follow Event!");
                self.handle_follow_event(follow_event, conversation).await?;
            }
            TwitchEvent::ChannelSubscribe(sub_event) => {
                println!("Channel Subscribe Event!");
                self.handle_subscribe_event(sub_event, conversation).await?;
            }
            TwitchEvent::ChannelRaid(raid_event) => {
                println!("Channel Raid Event!");
                self.handle_raid_event(raid_event, conversation).await?;
            }
            TwitchEvent::ChannelSubGift(sub_gift) => {
                println!("Channel Sub Gift Event!");
                self.handle_gift_sub_event(sub_gift, conversation).await?;
            }
        }
        Ok(())
    }

    pub async fn get_story_segment(
        &self,
        user_id: i64,
        event_type: String,
    ) -> anyhow::Result<String> {
        let mut conn = self.sqlite_pool.acquire().await?;
        let db_results = sqlite::get_latest_story_segments_for_user(conn, user_id).await?;
        Ok(db_results)
    }

    pub async fn handle_gift_sub_event(
        &self,
        gift_sub_event: &ChannelGiftMessage,
        mut conversation: Conversation,
    ) -> anyhow::Result<()> {
        let gifter_name = match gift_sub_event.user_name.clone() {
            Some(name) => name,
            None => "anonymous".to_string(),
        };

        let tier = match gift_sub_event.tier.clone() {
            NullSubTier::Tier1(tier) => tier,
            NullSubTier::Tier2(tier) => tier,
            NullSubTier::Tier3(tier) => tier,
            NullSubTier::Prime(tier) => tier,
            NullSubTier::Other(tier) => tier,
        };

        let response = conversation
            .send_message(format!(
                "tell me an epic story about how {} gifted new {} powers to {} null party members.",
                gifter_name, tier, gift_sub_event.total,
            ))
            .await?;

        println!("Response: {}", response.message().content);
        let mut conn = self.sqlite_pool.acquire().await?;
        let db_results = sqlite::write_new_gift_subs_event(
            conn,
            gift_sub_event,
            tier,
            response.message().content.to_string(),
        )
        .await?;
        println!("db_results: {:?}", db_results);

        let display_time = response.message().content.split(" ").count() * 500;

        let display_message = DisplayMessage {
            message: response.message().content.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelSubGift(gift_sub_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_raid_event(
        &self,
        raid_event: &RaidEvent,
        mut conversation: Conversation,
    ) -> anyhow::Result<()> {
        let response = conversation
            .send_message(format!(
                "tell me an epic story about how {} people from {}'s party joined forces with the Null party for a joint quest.",
                raid_event.viewers,
                raid_event.from_broadcaster_user_name,
            ))
            .await?;

        println!("Response: {}", response.message().content);
        let conn = self.sqlite_pool.acquire().await?;
        let db_results =
            sqlite::write_new_raid_event(conn, raid_event, response.message().content.to_string())
                .await?;
        println!("db_results: {:?}", db_results);

        let display_time = response.message().content.split(" ").count() * 500;

        let display_message = DisplayMessage {
            message: response.message().content.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelRaid(raid_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_subscribe_event(
        &self,
        subscriber_event: &SubscribeEvent,
        mut conversation: Conversation,
    ) -> anyhow::Result<()> {
        let response = conversation
            .send_message(format!(
                "tell me an epic story about how {} supported the party",
                subscriber_event.user_name
            ))
            .await?;

        println!("Response: {}", response.message().content);
        let conn = self.sqlite_pool.acquire().await?;
        let db_results = sqlite::write_new_story_segment(
            conn,
            subscriber_event.user_id,
            "subscribe".to_string(),
            response.message().content.to_string(),
        )
        .await?;
        println!("db_results: {:?}", db_results);

        let display_time = response.message().content.split(" ").count() * 500;

        let display_message = DisplayMessage {
            message: response.message().content.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelSubscribe(subscriber_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_follow_event(
        &self,
        follow_event: &FollowEvent,
        mut conversation: Conversation,
    ) -> anyhow::Result<()> {
        let response = conversation
            .send_message(format!(
                "tell me an epic story about how {} joined forces with the null party.",
                follow_event.user_name
            ))
            .await?;

        println!("Response: {}", response.message().content);
        let conn = self.sqlite_pool.acquire().await?;
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
            display_time,
            payload: TwitchEvent::ChannelFollow(follow_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }
}
