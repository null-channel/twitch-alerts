use std::sync::Arc;
use std::time::Duration;

use crate::messages::{
    ChannelGiftMessage, CheerEvent, FollowEvent, NewTwitchEventMessage, NullSubTier, RaidEvent,
    SubscribeEvent, TwitchEvent,
};
use anyhow::Context;
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use tokio_tungstenite::tungstenite;
use tracing::Instrument;
use twitch_api::eventsub::channel::{
    ChannelCheerV1, ChannelCheerV1Payload, ChannelFollowV2, ChannelFollowV2Payload, ChannelRaidV1,
    ChannelRaidV1Payload, ChannelSubscribeV1, ChannelSubscribeV1Payload, ChannelSubscriptionGiftV1,
    ChannelSubscriptionGiftV1Payload, ChannelSubscriptionMessageV1,
    ChannelSubscriptionMessageV1Payload,
};
use twitch_api::eventsub::{
    event::websocket::{EventsubWebsocketData, ReconnectPayload, SessionData, WelcomePayload},
    Event, EventSubscription, Message, Payload, Transport,
};
use twitch_api::helix::{ClientRequestError, HelixRequestPostError};
use twitch_api::twitch_oauth2::{Scope, TwitchToken, UserToken};
use twitch_api::{types, HelixClient};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Twitch sends a keepalive at least every 10 seconds; after this long without any frame the
/// connection is treated as dead.
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(40);
/// Pause between failed reconnection attempts.
const RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Listens to Twitch EventSub over WebSocket and forwards supported events to `sender`.
///
/// The listener never panics on protocol events: closed or failed connections are
/// reestablished, unsupported or malformed notifications are logged and skipped, and
/// subscriptions that already exist are treated as success.
#[derive(Clone)]
pub struct WebsocketClient {
    pub session_id: Option<String>,
    pub token: Arc<RwLock<UserToken>>,
    pub client: HelixClient<'static, reqwest::Client>,
    pub user_id: types::UserId,
    /// The EventSub WebSocket endpoint used for fresh sessions.
    pub connect_url: url::Url,
    /// A one-time URL Twitch asked us to move to. Subscriptions carry over to that session.
    pub reconnect_url: Option<url::Url>,
    pub sender: UnboundedSender<NewTwitchEventMessage>,
}

/// What the read loop should do after handling a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Reconnect,
}

impl WebsocketClient {
    /// Connect to the pending reconnect URL if Twitch supplied one, otherwise to the base URL.
    pub async fn connect(&mut self) -> anyhow::Result<Socket> {
        let url = self
            .reconnect_url
            .take()
            .unwrap_or_else(|| self.connect_url.clone());
        tracing::info!(%url, "connecting to twitch");
        let (socket, _) = tokio_tungstenite::connect_async_with_config(&url, None, false)
            .await
            .context("Can't connect")?;
        Ok(socket)
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut socket = self
            .connect()
            .await
            .context("when establishing connection")?;
        loop {
            let flow = tokio::select! {
                frame = futures::StreamExt::next(&mut socket) => match frame {
                    Some(Ok(message)) => {
                        let span = tracing::info_span!("processing message");
                        self.process_message(message).instrument(span).await?
                    }
                    Some(Err(error)) => {
                        tracing::warn!(%error, "Twitch EventSub connection failed, reestablishing it");
                        Flow::Reconnect
                    }
                    None => {
                        tracing::warn!("Twitch EventSub connection ended, reestablishing it");
                        Flow::Reconnect
                    }
                },
                _ = tokio::time::sleep(KEEPALIVE_TIMEOUT) => {
                    tracing::warn!("keepalive timeout, reestablishing connection");
                    Flow::Reconnect
                }
            };
            if flow == Flow::Reconnect {
                if let Some(reconnected) = self.reconnect().await {
                    socket = reconnected;
                }
            }
        }
    }

    /// Try once to open a new connection; on failure wait briefly and keep the current socket.
    async fn reconnect(&mut self) -> Option<Socket> {
        let span = tracing::info_span!("reconnect");
        match self.connect().instrument(span).await {
            Ok(socket) => Some(socket),
            Err(error) => {
                tracing::error!("failed to reestablish connection: {error:#}");
                tokio::time::sleep(RECONNECT_DELAY).await;
                None
            }
        }
    }

    pub async fn process_message(&mut self, msg: tungstenite::Message) -> anyhow::Result<Flow> {
        match msg {
            tungstenite::Message::Text(s) => {
                tracing::info!("{s}");
                let data = match Event::parse_websocket(&s) {
                    Ok(data) => data,
                    Err(error) => {
                        tracing::warn!(%error, "ignoring unparseable Twitch EventSub frame");
                        return Ok(Flow::Continue);
                    }
                };
                match data {
                    EventsubWebsocketData::Welcome {
                        payload: WelcomePayload { session },
                        ..
                    } => {
                        if let Err(error) = self.process_welcome_message(session).await {
                            // Twitch drops sessions without subscriptions, so start over.
                            tracing::error!("failed to subscribe to Twitch events: {error:#}");
                            return Ok(Flow::Reconnect);
                        }
                        Ok(Flow::Continue)
                    }
                    EventsubWebsocketData::Reconnect {
                        payload: ReconnectPayload { session },
                        ..
                    } => {
                        self.session_id = Some(session.id.to_string());
                        match session.reconnect_url {
                            Some(url) => {
                                self.reconnect_url =
                                    Some(url.parse().context("parse Twitch reconnect URL")?);
                                tracing::info!("Twitch asked us to move to a new session");
                                Ok(Flow::Reconnect)
                            }
                            None => {
                                tracing::warn!(
                                    "Twitch reconnect request without a URL; waiting for the close"
                                );
                                Ok(Flow::Continue)
                            }
                        }
                    }
                    EventsubWebsocketData::Notification { metadata, payload } => {
                        match new_twitch_event(payload) {
                            Ok(Some(event)) => {
                                let message = NewTwitchEventMessage {
                                    event,
                                    message_at: metadata.message_timestamp.as_str().into(),
                                    message_id: metadata.message_id.to_string(),
                                };
                                self.sender
                                    .send(message)
                                    .context("Twitch event receiver closed")?;
                            }
                            Ok(None) => tracing::info!(
                                event_type = ?metadata.subscription_type,
                                "ignoring unsupported Twitch event"
                            ),
                            Err(error) => tracing::warn!(
                                %error,
                                event_type = ?metadata.subscription_type,
                                "dropping malformed Twitch event"
                            ),
                        }
                        Ok(Flow::Continue)
                    }
                    EventsubWebsocketData::Revocation { metadata, .. } => {
                        tracing::warn!(?metadata, "Twitch revoked an EventSub subscription");
                        Ok(Flow::Continue)
                    }
                    EventsubWebsocketData::Keepalive { .. } => Ok(Flow::Continue),
                    _ => Ok(Flow::Continue),
                }
            }
            tungstenite::Message::Close(frame) => {
                tracing::warn!(?frame, "Twitch closed the EventSub connection");
                Ok(Flow::Reconnect)
            }
            _ => Ok(Flow::Continue),
        }
    }

    pub async fn process_welcome_message(&mut self, data: SessionData<'_>) -> anyhow::Result<()> {
        self.session_id = Some(data.id.to_string());
        let transport = Transport::websocket(data.id.clone());
        let user = self.user_id.clone();

        self.subscribe(ChannelFollowV2::new(user.clone(), user.clone()), &transport)
            .await?;
        self.subscribe(
            ChannelSubscribeV1::broadcaster_user_id(user.clone()),
            &transport,
        )
        .await?;
        self.subscribe(
            ChannelRaidV1::to_broadcaster_user_id(user.clone()),
            &transport,
        )
        .await?;
        self.subscribe(
            ChannelSubscriptionMessageV1::broadcaster_user_id(user.clone()),
            &transport,
        )
        .await?;
        self.subscribe(
            ChannelSubscriptionGiftV1::broadcaster_user_id(user.clone()),
            &transport,
        )
        .await?;
        if self.token.read().await.scopes().contains(&Scope::BitsRead) {
            self.subscribe(ChannelCheerV1::broadcaster_user_id(user), &transport)
                .await?;
        } else {
            tracing::warn!(
                "Twitch token lacks the bits:read scope; cheer alerts stay off until you authorize again"
            );
        }

        tracing::info!("we are listening");
        Ok(())
    }

    /// Create one EventSub subscription on the current session. A subscription that already
    /// exists (HTTP 409) counts as success so a reconnected session can be set up again.
    async fn subscribe<E>(&self, subscription: E, transport: &Transport) -> anyhow::Result<()>
    where
        E: EventSubscription + Send,
    {
        let token = self.token.read().await;
        match self
            .client
            .create_eventsub_subscription(subscription, transport.clone(), &*token)
            .await
        {
            Ok(_) => Ok(()),
            Err(ClientRequestError::HelixRequestPostError(HelixRequestPostError::Error {
                status,
                ..
            })) if status.as_u16() == 409 => {
                tracing::debug!(event_type = ?E::EVENT_TYPE, "EventSub subscription already exists");
                Ok(())
            }
            Err(error) => Err(error)
                .with_context(|| format!("create {:?} EventSub subscription", E::EVENT_TYPE)),
        }
    }
}

/// Convert a Twitch notification into an alert event.
///
/// Returns `Ok(None)` for event types this listener does not turn into alerts.
fn new_twitch_event(payload: Event) -> anyhow::Result<Option<TwitchEvent>> {
    let event = match payload {
        Event::ChannelFollowV2(Payload {
            message:
                Message::Notification(ChannelFollowV2Payload {
                    user_name, user_id, ..
                }),
            ..
        }) => TwitchEvent::ChannelFollow(FollowEvent {
            user_name: user_name.to_string(),
            user_id: user_id.to_string().parse::<i64>()?,
        }),
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
            tracing::info!("new subscription event");
            TwitchEvent::ChannelSubscribe(SubscribeEvent {
                user_name: user_name.to_string(),
                user_id: user_id.to_string().parse::<i64>()?,
                broadcaster_user_id: broadcaster_user_id.to_string().parse::<i64>()?,
                broadcaster_user_name: broadcaster_user_name.to_string(),
                is_gift,
                tier: subscription_tier(tier),
                streak_months: Some(1),
                cumulative_months: 1,
                duration_months: 1,
                message: "".to_string(),
            })
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
        }) => TwitchEvent::ChannelRaid(RaidEvent {
            from_broadcaster_user_id: from_broadcaster_user_id.to_string(),
            from_broadcaster_user_login: from_broadcaster_user_login.to_string(),
            from_broadcaster_user_name: from_broadcaster_user_name.to_string(),
            to_broadcaster_user_id: to_broadcaster_user_id.to_string(),
            to_broadcaster_user_login: to_broadcaster_user_login.to_string(),
            to_broadcaster_user_name: to_broadcaster_user_name.to_string(),
            viewers,
        }),
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
        }) => TwitchEvent::ChannelSubGift(ChannelGiftMessage {
            broadcaster_user_id: broadcaster_user_id.to_string(),
            broadcaster_user_login: broadcaster_user_login.to_string(),
            broadcaster_user_name: broadcaster_user_name.to_string(),
            cumulative_total,
            is_anonymous,
            tier: subscription_tier(tier),
            total,
            user_id: optional_string(user_id),
            user_login: optional_string(user_login),
            user_name: optional_string(user_name),
        }),
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
            tracing::info!("new resubscription event");
            TwitchEvent::ChannelResubscribe(SubscribeEvent {
                user_name: user_name.to_string(),
                user_id: user_id.to_string().parse::<i64>()?,
                broadcaster_user_id: broadcaster_user_id.to_string().parse::<i64>()?,
                broadcaster_user_name: broadcaster_user_name.to_string(),
                is_gift: false,
                tier: subscription_tier(tier),
                cumulative_months,
                duration_months,
                //TODO: deal with emotes
                message: message.text,
                streak_months,
            })
        }
        // Anonymous cheers carry no user; Twitch sets both fields to null.
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
        }) => TwitchEvent::ChannelCheer(CheerEvent {
            user_name: user_name
                .map(|name| name.to_string())
                .unwrap_or_else(|| "Anonymous".to_string()),
            user_id: user_id
                .map(|id| id.to_string().parse::<i64>())
                .transpose()?,
            bits,
            message: message.to_string(),
        }),
        _ => return Ok(None),
    };
    Ok(Some(event))
}

fn subscription_tier(tier: types::SubscriptionTier) -> NullSubTier {
    match tier {
        types::SubscriptionTier::Tier1 => NullSubTier::Tier1("rare".to_string()),
        types::SubscriptionTier::Tier2 => NullSubTier::Tier2("epic".to_string()),
        types::SubscriptionTier::Tier3 => NullSubTier::Tier3("legendary".to_string()),
        types::SubscriptionTier::Other(i) => NullSubTier::Other(i),
        types::SubscriptionTier::Prime => NullSubTier::Prime("prime".to_string()),
    }
}

fn optional_string<T: ToString>(input: Option<T>) -> Option<String> {
    input.map(|value| value.to_string())
}
