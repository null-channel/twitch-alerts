use crate::UnitedStates;
use axum::{extract::State, http::StatusCode};
use maud::{html, Markup};

#[derive(askama::Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub hostname: String,
    pub port: u16,
}

#[derive(askama::Template)]
#[template(path = "admin.html")]
pub struct AdminTemplate {
    pub enabled: bool,
    pub hostname: String,
    pub port: u16,
}

pub async fn index(State(sw_state): State<UnitedStates>) -> IndexTemplate {
    IndexTemplate {
        hostname: sw_state.host_info.websocket_host,
        port: sw_state.host_info.ws_port,
    }
}

pub async fn admin(State(sw_state): State<UnitedStates>) -> AdminTemplate {
    AdminTemplate {
        enabled: crate::types::EVENT_QUEUE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst),
        hostname: sw_state.host_info.websocket_host,
        port: sw_state.host_info.ws_port,
    }
}

pub async fn get_latest_unpublished_events(
    State(state): State<UnitedStates>,
) -> Result<Markup, (StatusCode, String)> {
    let queues = state.event_queues.lock().unwrap();
    let range = if queues.unpublished_events.len() < 10 {
        0..queues.unpublished_events.len()
    } else {
        0..10
    };
    let events = queues.unpublished_events.range(range);

    let class = if crate::types::EVENT_QUEUE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
        "running"
    } else {
        "paused"
    };
    Ok(html! {
        ul class=(class) {
            @for event in events {
                li { (event.message) }
            }
        }
    })
}

pub async fn get_latest_events(
    State(state): State<UnitedStates>,
) -> Result<Markup, (StatusCode, String)> {
    let queues = state.event_queues.lock().unwrap();
    let events = queues.latest_events.clone();

    Ok(html! {
        ul class="running" {
            @for event in events {
                li { (event.message) }
            }
        }
    })
}

pub async fn pause_events() -> Result<Markup, (StatusCode, String)> {
    crate::types::EVENT_QUEUE_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(html! {
        button id="event-queue-toggle" hx-get="/events/start" hx-swap="outerHTML" hx-target="#event-queue-toggle" { "Start" }
    })
}

pub async fn resume_events() -> Result<Markup, (StatusCode, String)> {
    crate::types::EVENT_QUEUE_ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(html! {
        button id="event-queue-toggle" hx-get="/events/pause" hx-swap="outerHTML" hx-target="#event-queue-toggle" { "Pause" }
    })
}

pub async fn get_all_events_in_queue(
    State(state): State<UnitedStates>,
) -> Result<Markup, (StatusCode, String)> {
    let queues = state.event_queues.lock().unwrap();
    let events = queues.unpublished_events.clone();
    Ok(html! {
        ul {
            @for event in events {
                li { (event.message) }
            }
        }
    })
}
