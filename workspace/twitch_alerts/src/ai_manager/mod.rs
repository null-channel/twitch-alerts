mod openai;
pub mod sqlite;

use crate::messages::{
    ChannelGiftMessage, CheerEvent, DisplayMessage, FollowEvent, NewTwitchEventMessage,
    NullSubTier, RaidEvent, SubscribeEvent, TwitchEvent,
};
use openai::StoryGenerator;
use tokio::sync::mpsc;

pub struct AIManager {
    pub sqlite_pool: sqlx::SqlitePool,
    story_generator: StoryGenerator,
    pub frontend_sender: mpsc::UnboundedSender<DisplayMessage>,
}

impl AIManager {
    pub fn new(
        sqlite: sqlx::SqlitePool,
        chat_key: String,
        model: String,
        fs: mpsc::UnboundedSender<DisplayMessage>,
    ) -> anyhow::Result<Self> {
        let story_generator = StoryGenerator::new(chat_key, model)?;
        Ok(AIManager {
            sqlite_pool: sqlite,
            story_generator,
            frontend_sender: fs,
        })
    }

    pub async fn run(
        &self,
        mut receiver: mpsc::UnboundedReceiver<NewTwitchEventMessage>,
    ) -> anyhow::Result<()> {
        loop {
            let msg = receiver.recv().await;

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
                None => return Err(anyhow::anyhow!("error: receiver closed")),
            }
        }
    }

    async fn new_event(&self, msg: NewTwitchEventMessage) -> anyhow::Result<()> {
        match &msg.event {
            TwitchEvent::ChannelFollow(follow_event) => {
                println!("Channel Follow Event!");
                self.handle_follow_event(follow_event).await?;
            }
            TwitchEvent::ChannelSubscribe(sub_event) => {
                println!("Channel Subscribe Event!");
                self.handle_subscribe_event(sub_event).await?;
            }
            TwitchEvent::ChannelRaid(raid_event) => {
                println!("Channel Raid Event!");
                self.handle_raid_event(raid_event).await?;
            }
            TwitchEvent::ChannelSubGift(sub_gift) => {
                println!("Channel Sub Gift Event!");
                self.handle_gift_sub_event(sub_gift).await?;
            }
            TwitchEvent::ChannelResubscribe(resub_event) => {
                println!("Channel Resub Event!");
                //TODO: handle resubscribe event
                self.handle_resub_event(resub_event).await?;
            }
            TwitchEvent::ChannelCheer(cheer_event) => {
                println!("Channel Cheer Event!");
                self.handle_cheer_event(cheer_event).await?;
            }
        }
        Ok(())
    }

    pub async fn handle_gift_sub_event(
        &self,
        gift_sub_event: &ChannelGiftMessage,
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

        let response = self
            .story_generator
            .generate(format!(
                "tell me an epic story about how {} gifted new {} powers to {} null party members.",
                gifter_name, tier, gift_sub_event.total,
            ))
            .await?;

        println!("Response: {}", response);
        let conn = self.sqlite_pool.acquire().await?;
        sqlite::write_new_gift_subs_event(conn, gift_sub_event, tier, response.to_string()).await?;

        let display_time = response.split_whitespace().count() * 500;

        let display_message = DisplayMessage {
            message: response.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelSubGift(gift_sub_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_raid_event(&self, raid_event: &RaidEvent) -> anyhow::Result<()> {
        let response = self.story_generator
            .generate(format!(
                "tell me an epic story about how {} people from {}'s party joined forces with the Null party for a joint quest.",
                raid_event.viewers,
                raid_event.from_broadcaster_user_name,
            ))
            .await?;

        println!("Response: {}", response);
        let conn = self.sqlite_pool.acquire().await?;
        sqlite::write_new_raid_event(conn, raid_event, response.to_string()).await?;

        let display_time = response.split_whitespace().count() * 500;

        let display_message = DisplayMessage {
            message: response.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelRaid(raid_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_resub_event(
        &self,
        subscriber_event: &SubscribeEvent,
    ) -> anyhow::Result<()> {
        let response = self
            .story_generator
            .generate(format!(
                "tell me an epic story about how {} supported the party",
                subscriber_event.user_name
            ))
            .await?;

        println!("Response: {}", response);
        let conn = self.sqlite_pool.acquire().await?;
        sqlite::write_new_story_segment(
            conn,
            subscriber_event.user_id,
            "subscribe".to_string(),
            response.to_string(),
        )
        .await?;

        let display_time = response.split_whitespace().count() * 500;

        let display_message = DisplayMessage {
            message: response.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelResubscribe(subscriber_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_subscribe_event(
        &self,
        subscriber_event: &SubscribeEvent,
    ) -> anyhow::Result<()> {
        let response = self
            .story_generator
            .generate(format!(
                "tell me an epic story about how {} supported the party",
                subscriber_event.user_name
            ))
            .await?;

        println!("Response: {}", response);
        let conn = self.sqlite_pool.acquire().await?;
        sqlite::write_new_story_segment(
            conn,
            subscriber_event.user_id,
            "subscribe".to_string(),
            response.to_string(),
        )
        .await?;

        let display_time = response.split_whitespace().count() * 500;

        let display_message = DisplayMessage {
            message: response.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelSubscribe(subscriber_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_cheer_event(&self, cheer_event: &CheerEvent) -> anyhow::Result<()> {
        let response = self
            .story_generator
            .generate(format!(
                "tell me an epic story about how {} threw {} gold coins to the null party.",
                cheer_event.user_name, cheer_event.bits
            ))
            .await?;

        println!("Response: {}", response);
        let conn = self.sqlite_pool.acquire().await?;
        // Anonymous cheers have no Twitch user, so they are stored under user id 0.
        sqlite::write_new_story_segment(
            conn,
            cheer_event.user_id.unwrap_or(0),
            "cheer".to_string(),
            response.to_string(),
        )
        .await?;

        let display_time = response.split_whitespace().count() * 500;

        let display_message = DisplayMessage {
            message: response.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelCheer(cheer_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }

    pub async fn handle_follow_event(&self, follow_event: &FollowEvent) -> anyhow::Result<()> {
        let response = self
            .story_generator
            .generate(format!(
                "tell me an epic story about how {} joined forces with the null party.",
                follow_event.user_name
            ))
            .await?;

        println!("Response: {}", response);
        let conn = self.sqlite_pool.acquire().await?;
        sqlite::write_new_story_segment(
            conn,
            follow_event.user_id,
            "follow".to_string(),
            response.to_string(),
        )
        .await?;

        let display_time = response.split_whitespace().count() * 750;
        //TODO: check if there is a "MAX_DISPLAY_TIME" env var

        let display_message = DisplayMessage {
            message: response.to_string(),
            image_url: "none".to_string(),
            sound_url: "none".to_string(),
            display_time,
            payload: TwitchEvent::ChannelFollow(follow_event.clone()),
        };
        self.frontend_sender.send(display_message)?;
        Ok(())
    }
}
