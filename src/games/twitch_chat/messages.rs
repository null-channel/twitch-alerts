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

pub struct Message {
    pub author: String,
    pub content: String,
}

#[derive(Default)]
pub struct Messages;

pub enum MessagePlatform {
    Twitch,
    Youtube,
    Discord,
}

pub struct ChatMessages {
    pub author: String,
    pub content: String,
    pub platform: MessagePlatform,
}

impl ChatMessages {
    pub fn new(author: String, content: String) -> Self {
        Self {
            author,
            content,
            platform: MessagePlatform::Twitch,
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
        state.thing.set("Hello, world!".to_string());
        if state.messages.len() > 20 {
            state.messages.pop_front();
        }
        let message_count = state.message_count.copy_value() + 1;
        state.message_count.set(message_count);
        state.messages.push_back(ChatMessageState {
            author: Value::new(message.author),
            content: Value::new(message.content),
            platform: Value::new(message.platform as i8),
        });
    }
}
