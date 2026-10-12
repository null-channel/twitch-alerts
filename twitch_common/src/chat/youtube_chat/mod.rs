//! YouTube live chat through the YouTube Data API v3.
//!
//! Only an API key is needed to read the chat of a public broadcast. The broadcast is
//! resolved once at start-up (`videos.list`, or `search.list` when only a channel id is
//! known) and its chat is then polled with `liveChatMessages.list`, honouring the polling
//! interval YouTube asks for. Every poll costs 5 quota units against the project's daily
//! quota (10,000 units by default), which is why the minimum interval is configurable.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::Context as _;
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::chat::messages::{Message, MessagePlatform, YoutubeChatMessage};
use crate::chat::source::ChatSource;

const API_BASE: &str = "https://www.googleapis.com/youtube/v3";
/// Below this YouTube rejects polls with `rateLimitExceeded` for most chats anyway.
pub const DEFAULT_MIN_POLL_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RESULTS: &str = "2000";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_BACKOFF: Duration = Duration::from_secs(5);
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// How to find the live broadcast whose chat to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Broadcast {
    /// The `v=` parameter of the broadcast's watch URL.
    VideoId(String),
    /// A channel id (`UC...`); its current live broadcast is looked up once at start-up.
    ChannelId(String),
}

#[derive(Debug, Clone)]
pub struct YoutubeChatConfig {
    /// A YouTube Data API v3 key from the Google Cloud console.
    pub api_key: String,
    pub broadcast: Broadcast,
    /// Lower bound on the time between polls. YouTube's own `pollingIntervalMillis` is
    /// honoured when it is longer.
    pub min_poll_interval: Duration,
}

/// A [`ChatSource`] for one YouTube live broadcast.
pub struct YoutubeChat {
    api: YoutubeApi,
    video_id: String,
    live_chat_id: String,
    min_poll_interval: Duration,
}

impl YoutubeChat {
    /// Resolve the broadcast's active live chat.
    ///
    /// Fails if the video cannot be found, is not live, or has chat disabled, so a
    /// misconfiguration is reported before a game takes over the terminal.
    pub async fn connect(config: YoutubeChatConfig) -> anyhow::Result<Self> {
        let api = YoutubeApi::new(config.api_key)?;
        let video_id = match config.broadcast {
            Broadcast::VideoId(video_id) => video_id,
            Broadcast::ChannelId(channel_id) => api
                .find_live_video(&channel_id)
                .await
                .context("search for the channel's live broadcast")?
                .with_context(|| format!("YouTube channel {channel_id} is not live right now"))?,
        };
        let video = api
            .video(&video_id)
            .await
            .context("look up the YouTube broadcast")?
            .with_context(|| format!("YouTube video {video_id} does not exist"))?;
        let title = video
            .snippet
            .as_ref()
            .map(|snippet| snippet.title.clone())
            .unwrap_or_default();
        let live_chat_id = video
            .live_streaming_details
            .and_then(|details| details.active_live_chat_id)
            .with_context(|| {
                format!("YouTube video {video_id} ({title}) is not live or has live chat disabled")
            })?;
        tracing::info!(video_id, title, "YouTube live chat resolved");
        Ok(Self {
            api,
            video_id,
            live_chat_id,
            min_poll_interval: config.min_poll_interval,
        })
    }

    pub fn video_id(&self) -> &str {
        &self.video_id
    }

    pub fn live_chat_id(&self) -> &str {
        &self.live_chat_id
    }
}

impl ChatSource for YoutubeChat {
    fn platform(&self) -> MessagePlatform {
        MessagePlatform::Youtube
    }

    fn name(&self) -> String {
        format!("youtube:{}", self.video_id)
    }

    fn run(
        self: Box<Self>,
        sender: UnboundedSender<Message>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        let poller = Poller {
            min_poll_interval: self.min_poll_interval,
            retry_backoff: RETRY_BACKOFF,
            max_consecutive_failures: MAX_CONSECUTIVE_FAILURES,
        };
        Box::pin(async move { poller.run(&self.api, &self.live_chat_id, &sender).await })
    }
}

/// Why a Data API request failed, with the cases the poll loop reacts to picked out.
#[derive(Debug)]
pub enum ApiError {
    /// The project's daily quota is used up; it resets at midnight Pacific time.
    QuotaExceeded,
    /// Polled faster than YouTube allows for this chat.
    RateLimited,
    /// The chat ended, is disabled or does not exist (`reason` says which).
    ChatUnavailable { reason: String },
    /// Any other error reported by the API.
    Api {
        status: u16,
        reason: String,
        message: String,
    },
    /// The request never got a usable answer: network failure, timeout, undecodable body.
    Transport(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QuotaExceeded => write!(f, "YouTube Data API daily quota exceeded"),
            Self::RateLimited => write!(f, "YouTube Data API rate limit exceeded"),
            Self::ChatUnavailable { reason } => {
                write!(f, "YouTube live chat unavailable ({reason})")
            }
            Self::Api {
                status,
                reason,
                message,
            } => write!(f, "YouTube Data API error {status} {reason}: {message}"),
            Self::Transport(message) => write!(f, "YouTube Data API request failed: {message}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    /// Classify a non-success response from its status and JSON error body.
    fn from_response(status: u16, body: &[u8]) -> Self {
        #[derive(Deserialize)]
        struct ErrorBody {
            error: ErrorDetail,
        }
        #[derive(Deserialize)]
        struct ErrorDetail {
            #[serde(default)]
            message: String,
            #[serde(default)]
            errors: Vec<ErrorItem>,
        }
        #[derive(Deserialize)]
        struct ErrorItem {
            #[serde(default)]
            reason: String,
        }

        let detail = match serde_json::from_slice::<ErrorBody>(body) {
            Ok(body) => body.error,
            Err(_) => {
                return Self::Api {
                    status,
                    reason: "unknown".into(),
                    message: String::from_utf8_lossy(body).chars().take(200).collect(),
                }
            }
        };
        let reason = detail
            .errors
            .into_iter()
            .map(|error| error.reason)
            .find(|reason| !reason.is_empty())
            .unwrap_or_else(|| "unknown".into());
        match reason.as_str() {
            "quotaExceeded" | "dailyLimitExceeded" => Self::QuotaExceeded,
            "rateLimitExceeded" => Self::RateLimited,
            "liveChatEnded" | "liveChatDisabled" | "liveChatNotFound" => {
                Self::ChatUnavailable { reason }
            }
            _ => Self::Api {
                status,
                reason,
                message: detail.message,
            },
        }
    }
}

/// The `liveChatMessages.list` call the poll loop depends on, separated so the loop can be
/// tested without the network. `streamList` could be added behind the same seam.
trait LiveChatApi {
    fn list_messages(
        &self,
        live_chat_id: &str,
        page_token: Option<&str>,
    ) -> impl Future<Output = Result<LiveChatPage, ApiError>> + Send;
}

/// A thin client for the handful of Data API calls live chat needs.
struct YoutubeApi {
    client: reqwest::Client,
    api_key: String,
}

impl YoutubeApi {
    fn new(api_key: String) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("bruce/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("build YouTube HTTP client")?;
        Ok(Self { client, api_key })
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, ApiError> {
        // The key travels in a header rather than the query string so it never ends up in a
        // logged URL.
        let response = self
            .client
            .get(format!("{API_BASE}/{path}"))
            .header("x-goog-api-key", &self.api_key)
            .query(query)
            .send()
            .await
            .map_err(transport_error)?;
        let status = response.status();
        let body = response.bytes().await.map_err(transport_error)?;
        if !status.is_success() {
            return Err(ApiError::from_response(status.as_u16(), &body));
        }
        serde_json::from_slice(&body)
            .map_err(|error| ApiError::Transport(format!("decode {path} response: {error}")))
    }

    /// The id of the channel's current live broadcast, if it is live. Costs 100 quota units.
    async fn find_live_video(&self, channel_id: &str) -> Result<Option<String>, ApiError> {
        let response: ListResponse<SearchItem> = self
            .get(
                "search",
                &[
                    ("part", "id"),
                    ("channelId", channel_id),
                    ("eventType", "live"),
                    ("type", "video"),
                    ("maxResults", "1"),
                ],
            )
            .await?;
        Ok(response.items.into_iter().find_map(|item| item.id.video_id))
    }

    /// The video's title and live streaming details, if the video exists. Costs 1 quota unit.
    async fn video(&self, video_id: &str) -> Result<Option<VideoItem>, ApiError> {
        let response: ListResponse<VideoItem> = self
            .get(
                "videos",
                &[("part", "snippet,liveStreamingDetails"), ("id", video_id)],
            )
            .await?;
        Ok(response.items.into_iter().next())
    }
}

fn transport_error(error: reqwest::Error) -> ApiError {
    ApiError::Transport(error.without_url().to_string())
}

impl LiveChatApi for YoutubeApi {
    fn list_messages(
        &self,
        live_chat_id: &str,
        page_token: Option<&str>,
    ) -> impl Future<Output = Result<LiveChatPage, ApiError>> + Send {
        let mut query = vec![
            ("liveChatId", live_chat_id.to_string()),
            ("part", "snippet,authorDetails".to_string()),
            ("maxResults", MAX_RESULTS.to_string()),
        ];
        if let Some(token) = page_token {
            query.push(("pageToken", token.to_string()));
        }
        async move {
            let query: Vec<(&str, &str)> = query
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect();
            self.get("liveChat/messages", &query).await
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListResponse<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    id: SearchId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchId {
    video_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoItem {
    snippet: Option<VideoSnippet>,
    live_streaming_details: Option<LiveStreamingDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoSnippet {
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveStreamingDetails {
    active_live_chat_id: Option<String>,
}

/// One `liveChatMessages.list` response.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveChatPage {
    next_page_token: Option<String>,
    /// How long YouTube wants us to wait before the next poll.
    #[serde(default)]
    polling_interval_millis: u64,
    /// Set once the broadcast has ended.
    offline_at: Option<String>,
    #[serde(default = "Vec::new")]
    items: Vec<LiveChatItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveChatItem {
    id: String,
    snippet: LiveChatSnippet,
    author_details: Option<AuthorDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveChatSnippet {
    #[serde(rename = "type")]
    kind: String,
    author_channel_id: Option<String>,
    display_message: Option<String>,
    text_message_details: Option<TextMessageDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextMessageDetails {
    message_text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorDetails {
    channel_id: Option<String>,
    display_name: Option<String>,
}

impl LiveChatPage {
    /// The ordinary chat messages on the page, oldest first. Membership, super chat and
    /// moderation events are not something a game can play with, so they are dropped.
    fn messages(&self) -> Vec<YoutubeChatMessage> {
        self.items
            .iter()
            .filter(|item| item.snippet.kind == "textMessageEvent")
            .filter_map(|item| {
                let author = item.author_details.as_ref();
                let author_channel_id = author
                    .and_then(|author| author.channel_id.clone())
                    .or_else(|| item.snippet.author_channel_id.clone())?;
                let text = item
                    .snippet
                    .text_message_details
                    .as_ref()
                    .and_then(|details| details.message_text.clone())
                    .or_else(|| item.snippet.display_message.clone())?;
                Some(YoutubeChatMessage {
                    id: item.id.clone(),
                    author_channel_id,
                    author_name: author
                        .and_then(|author| author.display_name.clone())
                        .unwrap_or_else(|| "YouTube viewer".into()),
                    text,
                })
            })
            .collect()
    }
}

/// Drives `liveChatMessages.list` until the chat ends, the receiver goes away, or a
/// failure that retrying cannot fix.
struct Poller {
    min_poll_interval: Duration,
    retry_backoff: Duration,
    max_consecutive_failures: u32,
}

impl Poller {
    async fn run<A: LiveChatApi>(
        &self,
        api: &A,
        live_chat_id: &str,
        sender: &UnboundedSender<Message>,
    ) -> anyhow::Result<()> {
        let mut page_token: Option<String> = None;
        let mut consecutive_failures = 0u32;
        let mut first_page = true;
        loop {
            let page = match api.list_messages(live_chat_id, page_token.as_deref()).await {
                Ok(page) => {
                    consecutive_failures = 0;
                    page
                }
                Err(ApiError::QuotaExceeded) => {
                    return Err(ApiError::QuotaExceeded)
                        .context("YouTube live chat polling stopped; the quota resets at midnight Pacific time");
                }
                Err(ApiError::ChatUnavailable { reason }) => {
                    tracing::info!(live_chat_id, reason, "YouTube live chat is over");
                    return Ok(());
                }
                Err(error) => {
                    consecutive_failures += 1;
                    if consecutive_failures > self.max_consecutive_failures {
                        return Err(error).context("YouTube live chat polling gave up");
                    }
                    tracing::warn!(
                        live_chat_id,
                        attempt = consecutive_failures,
                        %error,
                        "YouTube live chat poll failed; retrying"
                    );
                    tokio::time::sleep(self.retry_backoff).await;
                    continue;
                }
            };

            if first_page {
                // Like joining an IRC channel, a game starts from "now": the backlog YouTube
                // returns with the first page is not replayed.
                first_page = false;
                tracing::debug!(
                    skipped = page.items.len(),
                    "skipping YouTube chat history from before start-up"
                );
            } else {
                for message in page.messages() {
                    if sender.send(Message::new_youtube_message(message)).is_err() {
                        tracing::debug!(live_chat_id, "chat receiver dropped; stopping poll");
                        return Ok(());
                    }
                }
            }

            if let Some(offline_at) = &page.offline_at {
                tracing::info!(live_chat_id, offline_at, "YouTube broadcast went offline");
                return Ok(());
            }
            if page.next_page_token.is_some() {
                page_token = page.next_page_token;
            }
            let wait =
                Duration::from_millis(page.polling_interval_millis).max(self.min_poll_interval);
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    fn page(json: &str) -> LiveChatPage {
        serde_json::from_str(json).expect("page json")
    }

    #[test]
    fn only_text_messages_become_chat_messages() {
        let page = page(
            r#"{
              "nextPageToken": "t2",
              "pollingIntervalMillis": 3000,
              "items": [
                {"id": "a", "snippet": {"type": "textMessageEvent", "authorChannelId": "UC1",
                  "displayMessage": "crane", "textMessageDetails": {"messageText": "crane"}},
                  "authorDetails": {"channelId": "UC1", "displayName": "Alice"}},
                {"id": "b", "snippet": {"type": "newSponsorEvent", "authorChannelId": "UC2",
                  "displayMessage": "Bob became a member"},
                  "authorDetails": {"channelId": "UC2", "displayName": "Bob"}},
                {"id": "c", "snippet": {"type": "textMessageEvent", "authorChannelId": "UC3",
                  "displayMessage": "slate"}},
                {"id": "d", "snippet": {"type": "textMessageEvent", "displayMessage": "no author"}},
                {"id": "e", "snippet": {"type": "tombstone"}}
              ]
            }"#,
        );
        assert_eq!(page.next_page_token.as_deref(), Some("t2"));
        assert_eq!(page.polling_interval_millis, 3000);
        assert!(page.offline_at.is_none());
        assert_eq!(
            page.messages(),
            [
                YoutubeChatMessage {
                    id: "a".into(),
                    author_channel_id: "UC1".into(),
                    author_name: "Alice".into(),
                    text: "crane".into(),
                },
                YoutubeChatMessage {
                    id: "c".into(),
                    author_channel_id: "UC3".into(),
                    author_name: "YouTube viewer".into(),
                    text: "slate".into(),
                },
            ]
        );
    }

    #[test]
    fn api_errors_are_classified_by_reason() {
        let body = |reason: &str| {
            format!(
                r#"{{"error": {{"code": 403, "message": "nope", "errors": [{{"reason": "{reason}", "domain": "youtube"}}]}}}}"#
            )
        };
        assert!(matches!(
            ApiError::from_response(403, body("quotaExceeded").as_bytes()),
            ApiError::QuotaExceeded
        ));
        assert!(matches!(
            ApiError::from_response(403, body("rateLimitExceeded").as_bytes()),
            ApiError::RateLimited
        ));
        for reason in ["liveChatEnded", "liveChatDisabled", "liveChatNotFound"] {
            match ApiError::from_response(403, body(reason).as_bytes()) {
                ApiError::ChatUnavailable { reason: found } => assert_eq!(found, reason),
                other => panic!("{} classified as {:?}", reason, other),
            }
        }
        match ApiError::from_response(400, body("invalidPageToken").as_bytes()) {
            ApiError::Api {
                status,
                reason,
                message,
            } => {
                assert_eq!(status, 400);
                assert_eq!(reason, "invalidPageToken");
                assert_eq!(message, "nope");
            }
            other => panic!("unexpected {:?}", other),
        }
        match ApiError::from_response(502, b"<html>bad gateway</html>") {
            ApiError::Api { status, reason, .. } => {
                assert_eq!(status, 502);
                assert_eq!(reason, "unknown");
            }
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn video_and_search_responses_decode() {
        let video: ListResponse<VideoItem> = serde_json::from_str(
            r#"{"items": [{"id": "vid", "snippet": {"title": "Wordle night"},
                "liveStreamingDetails": {"actualStartTime": "2026-10-12T00:00:00Z", "activeLiveChatId": "chat-1"}}]}"#,
        )
        .unwrap();
        let item = video.items.into_iter().next().unwrap();
        assert_eq!(item.snippet.unwrap().title, "Wordle night");
        assert_eq!(
            item.live_streaming_details
                .unwrap()
                .active_live_chat_id
                .as_deref(),
            Some("chat-1")
        );

        let not_live: ListResponse<VideoItem> =
            serde_json::from_str(r#"{"items": [{"id": "vid", "snippet": {"title": "VOD"}}]}"#)
                .unwrap();
        assert!(not_live.items[0].live_streaming_details.is_none());

        let search: ListResponse<SearchItem> = serde_json::from_str(
            r#"{"items": [{"id": {"kind": "youtube#video", "videoId": "live-1"}}]}"#,
        )
        .unwrap();
        assert_eq!(search.items[0].id.video_id.as_deref(), Some("live-1"));
        let empty: ListResponse<SearchItem> = serde_json::from_str(r#"{"items": []}"#).unwrap();
        assert!(empty.items.is_empty());
    }

    /// Replays scripted responses and records the page tokens it was asked for.
    struct FakeApi {
        responses: Mutex<VecDeque<Result<LiveChatPage, ApiError>>>,
        tokens: Mutex<Vec<Option<String>>>,
    }

    impl FakeApi {
        fn new(responses: Vec<Result<LiveChatPage, ApiError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                tokens: Mutex::new(Vec::new()),
            }
        }

        fn tokens(&self) -> Vec<Option<String>> {
            self.tokens.lock().unwrap().clone()
        }
    }

    impl LiveChatApi for FakeApi {
        fn list_messages(
            &self,
            _live_chat_id: &str,
            page_token: Option<&str>,
        ) -> impl Future<Output = Result<LiveChatPage, ApiError>> + Send {
            self.tokens
                .lock()
                .unwrap()
                .push(page_token.map(str::to_string));
            let response = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(ApiError::Transport("script exhausted".into())));
            async move { response }
        }
    }

    fn text_page(token: &str, texts: &[&str], offline: bool) -> LiveChatPage {
        let items = texts
            .iter()
            .enumerate()
            .map(|(index, text)| {
                format!(
                    r#"{{"id": "{token}-{index}", "snippet": {{"type": "textMessageEvent", "authorChannelId": "UC{index}",
                        "textMessageDetails": {{"messageText": "{text}"}}}},
                        "authorDetails": {{"channelId": "UC{index}", "displayName": "viewer{index}"}}}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let offline = if offline {
            r#", "offlineAt": "2026-10-12T01:00:00Z""#
        } else {
            ""
        };
        page(&format!(
            r#"{{"nextPageToken": "{token}", "pollingIntervalMillis": 0, "items": [{items}]{offline}}}"#
        ))
    }

    fn fast_poller() -> Poller {
        Poller {
            min_poll_interval: Duration::ZERO,
            retry_backoff: Duration::ZERO,
            max_consecutive_failures: 2,
        }
    }

    async fn drain(receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Message>) -> Vec<String> {
        let mut texts = Vec::new();
        while let Ok(message) = receiver.try_recv() {
            texts.push(message.text());
        }
        texts
    }

    #[tokio::test]
    async fn backlog_is_skipped_then_messages_flow_until_offline() {
        let api = FakeApi::new(vec![
            Ok(text_page("t1", &["old1", "old2"], false)),
            Ok(text_page("t2", &["crane"], false)),
            Err(ApiError::Transport("blip".into())),
            Ok(text_page("t3", &["slate", "trace"], true)),
        ]);
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        fast_poller().run(&api, "chat", &sender).await.unwrap();
        assert_eq!(
            drain(&mut receiver).await,
            ["viewer0:crane", "viewer0:slate", "viewer1:trace"]
        );
        // The failed poll reuses the last token instead of starting over.
        assert_eq!(
            api.tokens(),
            [
                None,
                Some("t1".into()),
                Some("t2".into()),
                Some("t2".into())
            ]
        );
    }

    #[tokio::test]
    async fn quota_exhaustion_is_final() {
        let api = FakeApi::new(vec![
            Ok(text_page("t1", &[], false)),
            Err(ApiError::QuotaExceeded),
            Ok(text_page("t2", &["never"], false)),
        ]);
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let error = fast_poller().run(&api, "chat", &sender).await.unwrap_err();
        assert!(format!("{error:#}").contains("quota"), "{:#}", error);
        assert!(drain(&mut receiver).await.is_empty());
        assert_eq!(api.tokens().len(), 2);
    }

    #[tokio::test]
    async fn chat_ending_stops_quietly_and_repeated_failures_give_up() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let ended = FakeApi::new(vec![
            Ok(text_page("t1", &[], false)),
            Err(ApiError::ChatUnavailable {
                reason: "liveChatEnded".into(),
            }),
        ]);
        fast_poller().run(&ended, "chat", &sender).await.unwrap();

        let flaky = FakeApi::new(vec![
            Ok(text_page("t1", &[], false)),
            Err(ApiError::Transport("one".into())),
            Err(ApiError::RateLimited),
            Err(ApiError::Transport("three".into())),
        ]);
        let error = fast_poller()
            .run(&flaky, "chat", &sender)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("gave up"), "{:#}", error);
        assert_eq!(flaky.tokens().len(), 4);
    }

    #[tokio::test]
    async fn dropped_receiver_stops_polling() {
        let api = FakeApi::new(vec![
            Ok(text_page("t1", &[], false)),
            Ok(text_page("t2", &["crane"], false)),
            Ok(text_page("t3", &["slate"], false)),
        ]);
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        drop(receiver);
        fast_poller().run(&api, "chat", &sender).await.unwrap();
        assert_eq!(api.tokens().len(), 2);
    }
}
