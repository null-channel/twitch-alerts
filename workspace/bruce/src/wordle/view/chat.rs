use twitch_common::chat::messages::MessagePlatform;

use anathema::component::{Children, Component, Context};
use anathema::state::{List, State, Value};

#[derive(Default, State)]
pub struct MessagesState {
    messages: Value<List<ChatMessageState>>,
    message_count: Value<usize>,
    thing: Value<String>,
}

#[derive(Default, State)]
pub struct ChatMessageState {
    pub author: Value<String>,
    pub content: Value<String>,
    pub platform: Value<i8>,
}

#[derive(Default)]
pub struct Messages;

pub struct ChatMessages {
    pub author: String,
    pub content: String,
    pub platform: MessagePlatform,
}

impl ChatMessages {
    pub fn new(author: String, content: String, platform: MessagePlatform) -> Self {
        Self {
            author,
            content,
            platform,
        }
    }
}

impl Component for Messages {
    type Message = ChatMessages;
    type State = MessagesState;

    fn on_message(
        &mut self,
        message: Self::Message,
        state: &mut Self::State,
        _: Children<'_, '_>,
        _: Context<'_, '_, Self::State>,
    ) {
        let message_count = state.message_count.copy_value() + 1;
        state.message_count.set(message_count);
        state.messages.push_front(ChatMessageState {
            author: Value::new(message.author),
            content: Value::new(message.content),
            platform: Value::new(message.platform as i8),
        });
    }
}
