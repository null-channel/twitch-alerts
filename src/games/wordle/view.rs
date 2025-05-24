use chrono::Local;
use iocraft::prelude::*;
use std::{thread::JoinHandle, time::Duration};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver};

use super::game::WordleGameState;

#[derive(Default, Props)]
pub struct WordleGameProps {
    // gamestate message receiver
    pub gamestate_message_receiver: Option<UnboundedReceiver<WordleGameState>>,
    pub chat_message_receiver: Option<UnboundedReceiver<String>>,
}

#[component]
pub fn WordleGameView(mut hooks: Hooks, props: &WordleGameProps) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut time = hooks.use_state(|| Local::now());
    let mut should_exit = hooks.use_state(|| false);
    let mut chat_messages = vec!["Start of Chat".to_owned()];

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            time.set(Local::now());
        }
    });

    hooks.use_future(async move {
        let Some(cmr) = props.chat_message_receiver else {
            panic!("please give me a chat message receiver");
        };
        let Some(gsmr) = Some(props.gamestate_message_receiver) else {
            panic!("please give me a chat message receiver");
        };

        loop {
            let chat_message = cmr.try_recv();
        }
    });

    hooks.use_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent { code, kind, .. }) if kind != KeyEventKind::Release => {
                match code {
                    KeyCode::Char('q') => should_exit.set(true),
                    _ => {}
                }
            }
            _ => {}
        }
    });

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(
            width,
            height,
            background_color: Color::DarkGrey,
            border_style: BorderStyle::None,
            border_color: Color::Blue,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        ) {
            View(
                border_style: BorderStyle::None,
                border_color: Color::Blue,
                height: 100pct,
                width: 100pct,
                margin_bottom: 0,
                padding_top: 0,
                padding_bottom: 0,
                padding_left: 0,
                padding_right: 0,
            ) {
                View(
                    border_style: BorderStyle::Round,
                    border_color: Color::Blue,
                    height: 100pct,
                    width: 70pct,
                    margin_bottom: 2,
                    padding_top: 2,
                    padding_bottom: 2,
                    padding_left: 8,
                    padding_right: 8,
                ) {
                    Text(content: format!("Current Game Time: {}", time.get().format("%r")))
                }
                View(
                    border_style: BorderStyle::Round,
                    border_color: Color::Blue,
                    height: 100pct,
                    width: 30pct,
                    margin_bottom: 2,
                    padding_top: 2,
                    padding_bottom: 2,
                    padding_left: 8,
                    padding_right: 8,
                ) {
                    #(
                    chat_messages.iter().enumerate().map(|item| {
                        element!(View(
                                    border_style: BorderStyle::Round,
                                    border_color: Color::Blue,
                                    height: 100pct,
                                    width: 100pct,
                                    margin_bottom: 1,
                                    padding_top: 1,
                                ) {
                                Text(content: item.1.as_str())
                                }
                        )}
                    )
                    )}
            }
            Text(content: "Press \"q\" to quit.")
        }
    }
}

pub async fn run() {
    let mut system = element!(WordleGameView);
    //smol::block_on(element!(Example).fullscreen()).unwrap();
    let r = tokio::join!(system.fullscreen());
    if let Err(e) = r.0 {
        eprintln!("Error: {}", e);
    };
}
