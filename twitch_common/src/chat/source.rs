use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::chat::messages::{Message, MessagePlatform};

/// One connection to one chat platform.
///
/// A source is created (and typically connected) up front so configuration problems surface
/// before a game takes over the terminal, then driven to completion by [`ChatHub::spawn`].
pub trait ChatSource: Send + 'static {
    /// The platform this source reads from.
    fn platform(&self) -> MessagePlatform;

    /// Identifies this connection in logs, e.g. `twitch:#channel` or `youtube:<video id>`.
    fn name(&self) -> String;

    /// Forward every chat message to `sender` until the connection ends.
    ///
    /// Returning `Ok(())` means the chat ended normally (stream went offline, receiver
    /// dropped). Transient failures should be retried inside `run`; an `Err` is final.
    fn run(
        self: Box<Self>,
        sender: UnboundedSender<Message>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
}

/// Merges any number of [`ChatSource`]s into a single stream of [`Message`]s.
#[derive(Default)]
pub struct ChatHub {
    sources: Vec<Box<dyn ChatSource>>,
}

impl ChatHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<S: ChatSource>(&mut self, source: S) {
        self.sources.push(Box::new(source));
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// The platforms with at least one source, in the order they were added, without duplicates.
    pub fn platforms(&self) -> Vec<MessagePlatform> {
        let mut platforms: Vec<MessagePlatform> = Vec::new();
        for source in &self.sources {
            let platform = source.platform();
            if !platforms.contains(&platform) {
                platforms.push(platform);
            }
        }
        platforms
    }

    /// Run every source on its own task and return the merged message stream.
    ///
    /// A source that stops does not stop the others; its outcome is logged and announced on
    /// the stream as a [`Message::Debug`] so it shows up wherever chat is displayed. The
    /// receiver closes once every source has finished.
    pub fn spawn(self) -> UnboundedReceiver<Message> {
        let (sender, receiver) = mpsc::unbounded_channel();
        for source in self.sources {
            let sender = sender.clone();
            let name = source.name();
            tokio::spawn(async move {
                match source.run(sender.clone()).await {
                    Ok(()) => {
                        tracing::info!(source = %name, "chat source finished");
                        let _ =
                            sender.send(Message::new_debug_message(format!("{name} chat ended")));
                    }
                    Err(error) => {
                        tracing::error!(source = %name, error = %error, "chat source failed");
                        let _ = sender.send(Message::new_debug_message(format!(
                            "{name} chat stopped: {error}"
                        )));
                    }
                }
            });
        }
        receiver
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSource {
        platform: MessagePlatform,
        label: &'static str,
        messages: Vec<&'static str>,
        fail: bool,
    }

    impl ChatSource for FakeSource {
        fn platform(&self) -> MessagePlatform {
            self.platform
        }

        fn name(&self) -> String {
            self.label.to_string()
        }

        fn run(
            self: Box<Self>,
            sender: UnboundedSender<Message>,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
            Box::pin(async move {
                for text in self.messages {
                    sender.send(Message::new_debug_message(text.to_string()))?;
                }
                if self.fail {
                    anyhow::bail!("boom");
                }
                Ok(())
            })
        }
    }

    #[test]
    fn platforms_are_listed_once_in_insertion_order() {
        let mut hub = ChatHub::new();
        assert!(hub.is_empty());
        for (platform, label) in [
            (MessagePlatform::Youtube, "yt-1"),
            (MessagePlatform::Twitch, "tw"),
            (MessagePlatform::Youtube, "yt-2"),
        ] {
            hub.add(FakeSource {
                platform,
                label,
                messages: vec![],
                fail: false,
            });
        }
        assert_eq!(hub.len(), 3);
        assert_eq!(
            hub.platforms(),
            [MessagePlatform::Youtube, MessagePlatform::Twitch]
        );
    }

    #[tokio::test]
    async fn spawn_merges_sources_and_announces_how_each_stopped() {
        let mut hub = ChatHub::new();
        hub.add(FakeSource {
            platform: MessagePlatform::Twitch,
            label: "twitch:#a",
            messages: vec!["one", "two"],
            fail: false,
        });
        hub.add(FakeSource {
            platform: MessagePlatform::Youtube,
            label: "youtube:b",
            messages: vec!["three"],
            fail: true,
        });
        let mut receiver = hub.spawn();
        let mut received = Vec::new();
        while let Some(message) = receiver.recv().await {
            received.push(message.content());
        }
        received.sort();
        assert_eq!(
            received,
            [
                "one",
                "three",
                "twitch:#a chat ended",
                "two",
                "youtube:b chat stopped: boom",
            ]
        );
    }
}
