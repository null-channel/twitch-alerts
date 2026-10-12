//! Chat ingestion shared by every game.
//!
//! A [`ChatSource`] is one connection to one platform (a Twitch channel, a YouTube live
//! broadcast, ...). Every source turns platform messages into the platform-neutral
//! [`messages::Message`] and pushes them into a single channel owned by the [`ChatHub`], so a
//! game only ever consumes one stream of messages regardless of how many platforms feed it.
//!
//! Adding a platform means implementing [`ChatSource`] for its client and, if it carries
//! platform-specific data, adding a variant to [`messages::Message`].

pub mod messages;
pub mod source;
pub mod twitch_chat;
#[cfg(feature = "youtube_chat")]
pub mod youtube_chat;

pub use source::{ChatHub, ChatSource};
