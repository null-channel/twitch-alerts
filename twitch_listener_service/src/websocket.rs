use std::sync::Arc;
use std::time::Duration;

use eyre::Context;
use messages::{
    ChannelGiftMessage, CheerEvent, FollowEvent, NewTwitchEventMessage, RaidEvent, SubscribeEvent,
    TwitchEvent,
};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use tokio_tungstenite::tungstenite;
use tracing::Instrument;
use twitch_api::eventsub::channel::{
    ChannelCheerV1Payload, ChannelRaidV1Payload, ChannelSubscribeV1Payload,
    ChannelSubscriptionGiftV1Payload, ChannelSubscriptionMessageV1Payload,
};
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
        let (socket, _) =
            tokio_tungstenite::connect_async_with_config(&self.connect_url, None, false)
                .await
                .context("Can't connect")?;
        Ok(socket)
    }

    pub async fn run(&mut self) -> Result<(), eyre::Error> {
        let mut socket = self
            .connect()
            .await
            .context("when establishing connection")?;
        loop {
            tokio::select!(
            Some(msg) = futures::StreamExt::next(&mut socket) => {
                let span = tracing::info_span!("message received", raw_message = ?msg);
                match msg {
                    Err(tungstenite::Error::Protocol(
                        tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                    )) => {
                        tracing::warn!(
                            "connection was sent an unexpected frame or was reset, reestablishing it"
                        );
                        let s = self.process_failure(span).await;
                        if let Some(res) = s {
                            socket = res;
                        }
                    }
                    _ => {
                        let message = msg.context("when getting message")?;
                        let span = tracing::info_span!("processing message");
                        self.process_message(message).instrument(span).await?
                    }
                };
            }
            _ = tokio::time::sleep(Duration::from_secs(40)) => {
                let span = tracing::info_span!("keepalive");
                tracing::warn!("keepalive timeout, reestablishing connection");
                let s = self.process_failure(span).await;
                if let Some(res) = s {
                    socket = res;
                }
            })
        }
    }

    async fn process_failure(
        &mut self,
        span: tracing::Span,
    ) -> Option<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    > {
        let res = self
            .connect()
            .instrument(span)
            .await
            .context("when reestablishing connection");

        if let Err(e) = res {
            tracing::error!("failed to reestablish connection: {e}");
            tokio::time::sleep(Duration::from_secs(1)).await;
            return None;
        }
        Some(res.unwrap())
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
        let event = new_twitch_event(data)?;
        let message = NewTwitchEventMessage {
            event,
            message_at: metadata.message_timestamp.as_str().into(),
            message_id: metadata.message_id.to_string(),
        };
        self.sender.send(message).unwrap();
        Ok(())
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
        let transport = twitch_api::eventsub::Transport::websocket(data.id.clone());

        self.client
            .create_eventsub_subscription(
                twitch_api::eventsub::channel::ChannelSubscribeV1::broadcaster_user_id(
                    self.user_id.clone(),
                ),
                transport.clone(),
                &*self.token.read().await,
            )
            .await?;

        self.client
            .create_eventsub_subscription(
                twitch_api::eventsub::channel::ChannelRaidV1::to_broadcaster_user_id(
                    self.user_id.clone(),
                ),
                transport.clone(),
                &*self.token.read().await,
            )
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
            message:
                Message::Notification(ChannelSubscribeV1Payload {
                    user_name,
                    user_id,
                    broadcaster_user_id,
                    broadcaster_user_name,
                    is_gift,
                    tier,
                    ..
                }),
            ..
        }) => {
            println!("New sub event +!+!+!+!+!+!!+!+!+!+!+!+!+!+");
            Ok(TwitchEvent::ChannelSubscribe(SubscribeEvent {
                user_name: user_name.to_string(),
                user_id: user_id.to_string().parse::<i64>()?,
                broadcaster_user_id: broadcaster_user_id.to_string().parse::<i64>()?,
                broadcaster_user_name: broadcaster_user_name.to_string(),
                is_gift,
                tier: twitch_teir_to_teir(tier),
                streak_months: Some(1),
                cumulative_months: 1,
                duration_months: 1,
                message: "".to_string(),
            }))
        }
        Event::ChannelRaidV1(Payload {
            message:
                Message::Notification(ChannelRaidV1Payload {
                    from_broadcaster_user_id,
                    from_broadcaster_user_login,
                    from_broadcaster_user_name,
                    to_broadcaster_user_id,
                    to_broadcaster_user_login,
                    to_broadcaster_user_name,
                    viewers,
                    ..
                }),
            ..
        }) => Ok(TwitchEvent::ChannelRaid(RaidEvent {
            from_broadcaster_user_id: from_broadcaster_user_id.to_string(),
            from_broadcaster_user_login: from_broadcaster_user_login.to_string(),
            from_broadcaster_user_name: from_broadcaster_user_name.to_string(),
            to_broadcaster_user_id: to_broadcaster_user_id.to_string(),
            to_broadcaster_user_login: to_broadcaster_user_login.to_string(),
            to_broadcaster_user_name: to_broadcaster_user_name.to_string(),
            viewers,
        })),
        // Need to populate data from api because the gifter is not listed
        Event::ChannelSubscriptionGiftV1(Payload {
            message:
                Message::Notification(ChannelSubscriptionGiftV1Payload {
                    broadcaster_user_id,
                    broadcaster_user_login,
                    broadcaster_user_name,
                    cumulative_total,
                    is_anonymous,
                    tier,
                    total,
                    user_id,
                    user_login,
                    user_name,
                    ..
                }),
            ..
        }) => Ok(TwitchEvent::ChannelSubGift(ChannelGiftMessage {
            broadcaster_user_id: broadcaster_user_id.to_string(),
            broadcaster_user_login: broadcaster_user_login.to_string(),
            broadcaster_user_name: broadcaster_user_name.to_string(),
            cumulative_total,
            is_anonymous,
            tier: twitch_teir_to_teir(tier),
            total,
            user_id: braid_optional_to_string_optional(user_id),
            user_login: braid_optional_to_string_optional(user_login),
            user_name: braid_optional_to_string_optional(user_name),
        })),

        Event::ChannelSubscriptionMessageV1(Payload {
            message:
                Message::Notification(ChannelSubscriptionMessageV1Payload {
                    user_name,
                    user_id,
                    broadcaster_user_id,
                    broadcaster_user_name,
                    tier,
                    cumulative_months,
                    duration_months,
                    message,
                    streak_months,
                    ..
                }),
            ..
        }) => {
            println!("New sub event +!+!+!+!+!+!!+!+!+!+!+!+!+!+");
            Ok(TwitchEvent::ChannelResubscribe(SubscribeEvent {
                user_name: user_name.to_string(),
                user_id: user_id.to_string().parse::<i64>()?,
                broadcaster_user_id: broadcaster_user_id.to_string().parse::<i64>()?,
                broadcaster_user_name: broadcaster_user_name.to_string(),
                is_gift: false,
                tier: twitch_teir_to_teir(tier),
                cumulative_months,
                duration_months,
                //TODO: deal with emotes
                message: message.text,
                streak_months,
            }))
        }

        Event::ChannelCheerV1(Payload {
            message:
                Message::Notification(ChannelCheerV1Payload {
                    user_name,
                    user_id,
                    bits,
                    message,
                    ..
                }),
            ..
        }) => Ok(TwitchEvent::ChannelCheer(CheerEvent {
            user_name: user_name.unwrap().to_string(),
            user_id: user_id.unwrap().to_string().parse::<i64>()?,
            bits,
            message: message.to_string(),
        })),

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
        Event::ChannelBanV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelBanV1 is not supported")),
        Event::ChannelUnbanV1(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelUnbanV1 is not supported")),
        Event::ChannelUpdateV2(Payload {
            message: Message::Notification(..),
            ..
        }) => Err(eyre::eyre!("ChannelUpdateV1 is not supported")),

        _ => todo!(),
    }
}

fn twitch_teir_to_teir(twithc_teir: types::SubscriptionTier) -> messages::NullSubTier {
    match twithc_teir {
        types::SubscriptionTier::Tier1 => messages::NullSubTier::Tier1("rare".to_string()),
        types::SubscriptionTier::Tier2 => messages::NullSubTier::Tier2("epic".to_string()),
        types::SubscriptionTier::Tier3 => messages::NullSubTier::Tier3("legendary".to_string()),
        types::SubscriptionTier::Other(i) => messages::NullSubTier::Other(i),
        types::SubscriptionTier::Prime => messages::NullSubTier::Prime("prime".to_string()),
    }
}

fn braid_optional_to_string_optional<T: ToString>(input: Option<T>) -> Option<String> {
    match input {
        None => None,
        Some(thing) => Some(thing.to_string()),
    }
}
