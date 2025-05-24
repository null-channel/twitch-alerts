use iocraft::prelude::*;
use std::time::Duration;

#[component]
pub fn CountDown(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);
    let mut countdown = hooks.use_state(|| 5);

    // Return an empty view element instead of unit type
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            countdown -= 1;
            if countdown <= 0 {
                should_exit.set(true);
            }
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
                    width: 100pct,
                    margin_bottom: 2,
                    padding_top: 2,
                    padding_bottom: 2,
                    padding_left: 8,
                    padding_right: 8,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                ) {
                    //TODO: Big number would be awesome!
                    Text(
                        content: format!("Games starts in: {}", countdown))
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
