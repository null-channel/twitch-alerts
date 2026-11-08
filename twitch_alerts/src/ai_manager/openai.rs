use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde_json::json;

const STORY_PROMPT: &str = "You are D&DGPT, when answering any questions, you always answer with a short epic story as a dungeons and dragons dungeon master in 27 words or less.";

pub(super) struct StoryGenerator {
    client: reqwest::Client,
    key: String,
    model: String,
    endpoint: url::Url,
}

#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: String,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

impl StoryGenerator {
    pub(super) fn new(key: String, model: String) -> anyhow::Result<Self> {
        anyhow::ensure!(!model.trim().is_empty(), "OPENAI_MODEL must not be empty");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("build OpenAI HTTP client")?;
        tracing::info!(model = %model, "OpenAI alert story model configured");
        Ok(Self {
            client,
            key,
            model,
            endpoint: "https://api.openai.com/v1/chat/completions".parse()?,
        })
    }

    pub(super) async fn generate(&self, prompt: String) -> anyhow::Result<String> {
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.key)
            .json(&json!({
                "model": self.model,
                "reasoning_effort": "none",
                "max_completion_tokens": 128,
                "messages": [
                    { "role": "system", "content": STORY_PROMPT },
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .context("request OpenAI alert story")?;
        // Do not log API response bodies: they can contain user data or secrets.
        anyhow::ensure!(
            response.status().is_success(),
            "OpenAI alert story request failed with HTTP {} for model {}",
            response.status(),
            self.model
        );
        let response: Completion = response.json().await.context("parse OpenAI alert story")?;
        let choice = response
            .choices
            .into_iter()
            .next()
            .context("OpenAI returned no alert story")?;
        anyhow::ensure!(
            choice.finish_reason == "stop",
            "OpenAI alert story did not complete: {}",
            choice.finish_reason
        );
        let story = choice
            .message
            .content
            .context("OpenAI returned no alert text")?;
        let story = story.trim();
        anyhow::ensure!(!story.is_empty(), "OpenAI returned an empty alert story");
        Ok(story.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{HeaderMap, StatusCode},
        routing::post,
        Json, Router,
    };
    use serde_json::Value;
    use tokio::{net::TcpListener, sync::mpsc, task::JoinSet};

    use crate::{
        ai_manager::AIManager,
        messages::{FollowEvent, NewTwitchEventMessage, TwitchEvent},
    };

    async fn mock_openai(
        model: &str,
        status: StatusCode,
        response: Value,
    ) -> (url::Url, JoinSet<()>) {
        let model = model.to_owned();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |headers: HeaderMap, Json(request): Json<Value>| {
                let response = response.clone();
                let model = model.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer local-test-key");
                    assert_eq!(request["model"], model);
                    assert_eq!(request["reasoning_effort"], "none");
                    assert_eq!(request["max_completion_tokens"], 128);
                    assert_eq!(request["messages"][0]["role"], "system");
                    assert_eq!(request["messages"][0]["content"], STORY_PROMPT);
                    assert_eq!(request["messages"][1]["role"], "user");
                    assert!(request["messages"][1]["content"]
                        .as_str()
                        .unwrap()
                        .contains("LocalFollower"));
                    for parameter in [
                        "temperature",
                        "top_p",
                        "max_tokens",
                        "presence_penalty",
                        "frequency_penalty",
                    ] {
                        assert!(
                            request.get(parameter).is_none(),
                            "unexpected legacy parameter: {}",
                            parameter
                        );
                    }
                    (status, Json(response))
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        )
        .parse()
        .unwrap();
        let mut servers = JoinSet::new();
        servers.spawn(async move { axum::serve(listener, app).await.unwrap() });
        (endpoint, servers)
    }

    fn follow_event() -> NewTwitchEventMessage {
        NewTwitchEventMessage {
            event: TwitchEvent::ChannelFollow(FollowEvent {
                user_name: "LocalFollower".into(),
                user_id: 123,
            }),
            message_id: "local-follow".into(),
            message_at: "2026-10-18T12:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn configured_model_generates_a_story_that_is_saved_and_queued() {
        for model in ["gpt-6-luna", "custom-alert-model"] {
            let story = "A brave local adventurer joins the party.";
            let (endpoint, _servers) = mock_openai(model, StatusCode::OK, json!({
                "choices": [{"message": {"content": format!("  {story}\n")}, "finish_reason": "stop"}]
            })).await;
            let pool = sql_common::setup::setup_sqlite_migrations(
                "sqlite::memory:".into(),
                &crate::MIGRATIONS,
            )
            .await
            .unwrap();
            let (sender, mut receiver) = mpsc::unbounded_channel();
            let mut manager =
                AIManager::new(pool.clone(), "local-test-key".into(), model.into(), sender)
                    .unwrap();
            manager.story_generator.endpoint = endpoint;
            manager.new_event(follow_event()).await.unwrap();
            let saved: (i64, String, String) =
                sqlx::query_as("SELECT user_id, event_type, story_segment FROM story_segments")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(saved, (123, "follow".into(), story.into()));
            let display = receiver.try_recv().unwrap();
            assert_eq!(display.message, story);
            assert_eq!(display.display_time, story.split_whitespace().count() * 750);
            assert!(matches!(
                display.payload,
                TwitchEvent::ChannelFollow(FollowEvent { user_id: 123, .. })
            ));
        }
    }

    #[tokio::test]
    async fn failed_or_incomplete_generation_does_not_persist_or_queue_an_alert() {
        let cases = [
            (
                StatusCode::TOO_MANY_REQUESTS,
                json!({"error": {"message": "private-response-body"}}),
            ),
            (StatusCode::OK, json!({"choices": []})),
            (
                StatusCode::OK,
                json!({"choices": [{"message": {"content": "unfinished story"}, "finish_reason": "length"}]}),
            ),
            (
                StatusCode::OK,
                json!({"choices": [{"message": {"content": null}, "finish_reason": "stop"}]}),
            ),
            (
                StatusCode::OK,
                json!({"choices": [{"message": {"content": "  "}, "finish_reason": "stop"}]}),
            ),
        ];
        for (status, response) in cases {
            let (endpoint, _servers) = mock_openai("gpt-6-luna", status, response).await;
            let pool = sql_common::setup::setup_sqlite_migrations(
                "sqlite::memory:".into(),
                &crate::MIGRATIONS,
            )
            .await
            .unwrap();
            let (sender, mut receiver) = mpsc::unbounded_channel();
            let mut manager = AIManager::new(
                pool.clone(),
                "local-test-key".into(),
                "gpt-6-luna".into(),
                sender,
            )
            .unwrap();
            manager.story_generator.endpoint = endpoint;
            let error = manager.new_event(follow_event()).await.unwrap_err();
            assert!(!format!("{error:#}").contains("private-response-body"));
            let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM story_segments")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(rows, 0);
            assert!(receiver.try_recv().is_err());
        }
    }
}
