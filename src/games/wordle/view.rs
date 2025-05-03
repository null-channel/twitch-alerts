use chrono::Local;
use iocraft::prelude::*;
use std::{thread::JoinHandle, time::Duration};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Default, Props)]
struct WordleProps {
    // gamestate message receiver
    gamestate_message_receiver: Option<Receiver<String>>,
}

#[component]
fn Example(mut hooks: Hooks, props: &WordleProps) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut time = hooks.use_state(|| Local::now());
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            time.set(Local::now());
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
                    Text(content: "Chat")
                }
            }
            Text(content: "Press \"q\" to quit.")
        }
    }
}

pub async fn run() {
    let mut system = element!(Example);
    //smol::block_on(element!(Example).fullscreen()).unwrap();
    let r = tokio::join!(system.fullscreen());
    if let Err(e) = r.0 {
        eprintln!("Error: {}", e);
    };
}
