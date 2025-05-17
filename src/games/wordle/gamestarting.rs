use iocraft::prelude::*;
use std::time::Duration;
use tokio::sync::mpsc::Sender;

#[derive(Default, Props)]
pub struct CountdownProps {
    // gamestate message receiver
    pub gamestate_message_receiver: Option<Sender<bool>>,
    pub one_shot: Option<Sender<bool>>,
}

#[component]
pub fn CountDown(mut hooks: Hooks, props: &CountdownProps) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);
    let mut countdown = 5;

    let Some(sender) = props.one_shot.clone() else {
        // Return an empty view element instead of unit type
        return element! {
            View(
                width,
                height,
                background_color: Color::DarkGrey,
            ) {
                Text(content: "Error: Failed to get sender")
            }
        };
    };
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            countdown -= 1;
            if countdown <= 0 {
                if let Err(_) = sender.send(true).await {
                    eprintln!("Failed to send countdown message");
                }
            }
            break;
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
                    Text(content: format!("Current Game Time: {}", "marek"))
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
                    Text(content: format!("Countdown: {}", countdown))
                }
            }
            Text(content: "Press \"q\" to quit.")
        }
    }
}

pub async fn run() {
    let mut system = element!(CountDown);
    //smol::block_on(element!(Example).fullscreen()).unwrap();
    let r = tokio::join!(system.fullscreen());
    if let Err(e) = r.0 {
        eprintln!("Error: {}", e);
    };
}
