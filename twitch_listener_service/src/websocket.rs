use std::sync::Arc;

use eyre::Context;
use messages::{FollowEvent, NewTwitchEventMessage, TwitchEvent, SubscribeEvent};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use tokio_tungstenite::tungstenite;
use tracing::Instrument;
use twitch_api::eventsub::channel::ChannelSubscribeV1Payload;
use twitch_api::twitch_oauth2::UserToken;
use twitch_api::{
    eventsub::{
        channel::ChannelFollowV2Payload,
        event::websocket::{EventsubWebsocketData, ReconnectPayload, SessionData, WelcomePayload},
        Event, Message, NotificationMetadata, Payload,
    },
    types::{self},
    HelixClient,
};

#[derive(Clone)]
pub struct WebsocketClient {
    pub session_id: Option<String>,
    pub token: Arc<RwLock<UserToken>>,
    pub client: HelixClient<'static, reqwest::Client>,
    pub user_id: types::UserId,
    pub connect_url: url::Url,
    pub sender: UnboundedSender<NewTwitchEventMessage>,
}

impl WebsocketClient {
    pub async fn connect(
        &self,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        eyre::Error,
    > {
        tracing::info!("connecting to twitch");
        let config = tungstenite::protocol::WebSocketConfig {
            max_send_queue: None,
            max_message_size: Some(64 << 20), // 64 MiB
            max_frame_size: Some(16 << 20),   // 16 MiB
            accept_unmasked_frames: false,
        };
        let (socket, _) =
            tokio_tungstenite::connect_async_with_config(&self.connect_url, Some(config), false)
                .await
                .context("Can't connect")?;

        Ok(socket)
    }

    pub async fn run(mut self) -> Result<(), eyre::Error> {
        let mut s = self
            .connect()
            .await
            .context("when establishing connection")?;
        loop {
            tokio::select!(
            Some(msg) = futures::StreamExt::next(&mut s) => {
                let span = tracing::info_span!("message received", raw_message = ?msg);
                let msg = match msg {
                    Err(tungstenite::Error::Protocol(
                        tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                    )) => {
                        tracing::warn!(
                            "connection was sent an unexpected frame or was reset, reestablishing it"
                        );
                        s = self
                            .connect().instrument(span)
                            .await
                            .context("when reestablishing connection")?;
                        continue
                    }
                    _ => msg.context("when getting message")?,
                };
                self.process_message(msg).instrument(span).await?
            })
        }
    }

    pub async fn process_message(&mut self, msg: tungstenite::Message) -> Result<(), eyre::Report> {
        match msg {
            tungstenite::Message::Text(s) => {
                tracing::info!("{s}");
                match Event::parse_websocket(&s)? {
                    EventsubWebsocketData::Welcome {
                        payload: WelcomePayload { session },
                        ..
                    }
                    | EventsubWebsocketData::Reconnect {
                        payload: ReconnectPayload { session },
                        ..
                    } => {
                        self.process_welcome_message(session).await?;
                        Ok(())
                    }
                    EventsubWebsocketData::Notification { metadata, payload } => {
                        self.process_notification(payload, &metadata, &s)?;
                        Ok(())
                    }
                    EventsubWebsocketData::Revocation {
                        metadata: _,
                        payload: _,
                    } => Ok(()),
                    EventsubWebsocketData::Keepalive {
                        metadata: _,
                        payload: _,
                    } => Ok(()),
                    _ => Ok(()),
                }
            }
            tungstenite::Message::Close(_) => todo!(),
            _ => Ok(()),
        }
    }

    fn process_notification(
        &self,
        data: Event,
        metadata: &NotificationMetadata<'_>,
        _payload: &str,
    ) -> Result<(), eyre::Report> {
        // TODO: Delete as this is wrong... but is how it still works for right now!

        match &data {
            Event::ChannelFollowV2(Payload {
                message: Message::Notification(..),
                ..
            }) => {
                let event = new_twitch_event(data)?;
                let message = NewTwitchEventMessage {
                    event,
                    message_at: metadata.message_timestamp.as_str().into(),
                    message_id: metadata.message_id.to_string(),
                };
                self.sender.send(message).unwrap();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub async fn process_welcome_message(
        &mut self,
        data: SessionData<'_>,
    ) -> Result<(), eyre::Report> {
        self.session_id = Some(data.id.to_string());
        if let Some(url) = data.reconnect_url {
            self.connect_url = url.parse()?;
        }
        let req = twitch_api::helix::eventsub::CreateEventSubSubscriptionRequest::new();
        let body = twitch_api::helix::eventsub::CreateEventSubSubscriptionBody::new(
            twitch_api::eventsub::channel::ChannelFollowV2::new(
                self.user_id.clone(),
                self.user_id.clone(),
            ),
            twitch_api::eventsub::Transport::websocket(data.id.clone()),
        );

        self.client
            .req_post(req, body, &*self.token.read().await)
            .await?;
        tracing::info!("we are listening");
        Ok(())
    }
}

// Creates a new TwitchEvent enum from the payload and metadata
fn new_twitch_event(payload: Event) -> Result<TwitchEvent, eyre::Report> {
    match payload {
        Event::ChannelFollowV2(Payload {
            message:
                Message::Notification(ChannelFollowV2Payload {
                    user_name, user_id, ..
                }),
            ..
        }) => Ok(TwitchEvent::ChannelFollow(FollowEvent {
            user_name: user_name.to_string(),
            user_id: user_id.to_string().parse::<i64>()?,
        })),
        Event::ChannelSubscribeV1(Payload {
            message: Message::Notification(ChannelSubscribeV1Payload {
                user_name, user_id, broadcaster_user_id, broadcaster_user_name, is_gift, tier, ..
            }),
            ..
        }) => Ok(TwitchEvent::ChannelSubscribe(SubscribeEvent {
            user_name: user_name.to_string(),
            user_id: user_id.to_string().parse::<i64>()?,
            broadcaster_user_id: broadcaster_user_id.to_string().parse::<i64>()?,
            broadcaster_user_name: broadcaster_user_name.to_string(),
            is_gift,
            tier: twitch_teir_to_teir(tier),
        })),
        Event::ChannelCheerV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelCheerV1 is not supported")),
        Event::ChannelPointsCustomRewardAddV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!(
            "ChannelPointsCustomRewardAddV1 is not supported"
        )),
        Event::ChannelPointsCustomRewardUpdateV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!(
            "ChannelPointsCustomRewardUpdateV1 is not supported"
        )),
        Event::ChannelPointsCustomRewardRemoveV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!(
            "ChannelPointsCustomRewardRemoveV1 is not supported"
        )),
        Event::ChannelPointsCustomRewardRedemptionAddV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!(
            "ChannelPointsCustomRewardRedemptionAddV1 is not supported"
        )),
        Event::ChannelPointsCustomRewardRedemptionUpdateV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!(
            "ChannelPointsCustomRewardRedemptionUpdateV1 is not supported"
        )),
        Event::ChannelHypeTrainBeginV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelHypeTrainBeginV1 is not supported")),
        Event::ChannelHypeTrainProgressV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelHypeTrainProgressV1 is not supported")),
        Event::ChannelHypeTrainEndV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelHypeTrainEndV1 is not supported")),
        Event::ChannelPollBeginV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelPollBeginV1 is not supported")),
        Event::ChannelPollProgressV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelPollProgressV1 is not supported")),
        Event::ChannelPollEndV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelPollEndV1 is not supported")),
        Event::ChannelPredictionBeginV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelPredictionBeginV1 is not supported")),
        Event::ChannelPredictionProgressV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelPredictionProgressV1 is not supported")),
        Event::ChannelPredictionLockV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelPredictionLockV1 is not supported")),
        Event::ChannelPredictionEndV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelPredictionEndV1 is not supported")),
        Event::ChannelRaidV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelRaidV1 is not supported")),
        Event::ChannelBanV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelBanV1 is not supported")),
        Event::ChannelUnbanV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelUnbanV1 is not supported")),

        Event::ChannelUpdateV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelUpdateV1 is not supported")),
        _ => todo!(),
    }
}

fn twitch_teir_to_teir(twithc_teir: types::SubscriptionTier) -> messages::SubscriptionTier {
    match twithc_teir {
        types::SubscriptionTier::Tier1 => messages::SubscriptionTier::Tier1,
        types::SubscriptionTier::Tier2 => messages::SubscriptionTier::Tier2,
        types::SubscriptionTier::Tier3 => messages::SubscriptionTier::Tier3,
        types::SubscriptionTier::Other(i) => messages::SubscriptionTier::Other(i),
        types::SubscriptionTier::Prime => messages::SubscriptionTier::Prime,
    }
}