use ::futures::StreamExt;
use gloo::console::{self, Timer};
use gloo::timers::callback::{Interval, Timeout};
use messages::DisplayMessage;
use messages::TwitchEvent;
use reqwasm::websocket::{
    futures::{self, WebSocket},
    Message,
};
use std::sync::mpsc::{Receiver, Sender};
use web_sys::{AudioContext, HtmlAudioElement};
use ws_stream_wasm::{WsMessage, WsMeta};
use yew::platform::spawn_local;
use yew::prelude::*;

// A macro to provide `println!(..)`-style syntax for `console.log` logging.
macro_rules! log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    }
}
pub enum Msg {
    NewEventMsg,
    EventFinished,
    PollApi,
    WebsocketMessage(String),
}

pub struct App {
    _standalone: (Interval, Interval),
    event_queue: Vec<String>,
    current_message: Option<DisplayMessage>,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        // create websocket connection
        let wb_callback = ctx.link().callback(Msg::WebsocketMessage);

        let loc = gloo::utils::window().location();

        let host = loc.hostname();

        if let Ok(websocket_host) = host {
            listen_to_webhook(wb_callback, websocket_host);
        }
        // Run both futures to completion
        let standalone_handle = Interval::new(10000, || {
            console::debug!("Example of a standalone callback.")
        });

        let clock_handle = {
            let link = ctx.link().clone();
            Interval::new(30000, move || link.send_message(Msg::PollApi))
        };

        Self {
            _standalone: (standalone_handle, clock_handle),
            event_queue: Vec::new(),
            current_message: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::PollApi => {
                //poll api for new events
                let link = ctx.link().clone();
                link.send_message(Msg::NewEventMsg);
                true
            }
            Msg::NewEventMsg => {
                if self.current_message.is_some() {
                    log!("Current message is not finished yet.");
                    return true;
                }
                let message_json = self.event_queue.pop();

                log!("New event message: {:?}", message_json);
                if let Some(message_json) = message_json {
                    let message = serde_json::from_str::<DisplayMessage>(&message_json);

                    let Ok(message) = message else {
                        log!("Error parsing message: {}", message_json);
                        return true;
                    };

                    log!("New message with display time: {}", &message.display_time);
                    self.current_message = Some(message.clone());

                    let link = ctx.link().clone();
                    let timeout = Timeout::new(message.display_time as u32, move || {
                        link.send_message(Msg::EventFinished)
                    });
                    let result = web_sys::HtmlAudioElement::new_with_src("sound/dial-up-modem.wav");
                    let _ = result.unwrap().play();
                    Timeout::forget(timeout);
                }

                true
            }
            Msg::EventFinished => {
                self.current_message = None;
                if !self.event_queue.is_empty() {
                    log!("Event time expired, getting next event.");
                    let link = ctx.link().clone();
                    let _ = Timeout::new(100, move || {
                        link.send_message(Msg::NewEventMsg);
                    })
                    .forget();
                }
                true
            }
            Msg::WebsocketMessage(message) => {
                log!("Message from websocket: {}", message);
                println!("Message from websocket: {}", message);
                self.event_queue.push(message);
                let link = ctx.link().clone();
                link.send_message(Msg::NewEventMsg);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        //self.timeout.is_some() || self.interval.is_some();
        html! {
            <>
                <div id="wrapper">
                    if let Some(message) = &self.current_message {
                        <div id="clan_image">
                            <img src="img/null-logo.svg" alt="null logo"/>
                        </div>
                        <div id="messages">
                            {  html! { <p>{ message.message.as_str() }</p> } }
                        </div>
                        <div id="footer">
                            {
                                match &message.payload {
                                    TwitchEvent::ChannelFollow(follow) => {
                                        html! {<p>{ format!("Thank you {} for following", follow.user_name) }</p> }
                                    }
                                    TwitchEvent::ChannelSubscribe(sub) => {
                                        html! {<p>{ "Thank you " } <span id="sub">{sub.user_name.clone()}</span> { "for subscribing!!!" }</p> }
                                    }
                                    TwitchEvent::ChannelRaid(raid) => {
                                        html! {<p>{ format!("{} raided with {} viewers!", raid.from_broadcaster_user_name, raid.viewers) }</p> }
                                    }
                                    TwitchEvent::ChannelSubGift(gift) => {
                                        if let Some(gifter) = gift.clone().user_name {
                                            html! {<p>{ format!("{} gifted a sub to {}!", gifter, gift.broadcaster_user_name.to_string()) }</p> }
                                        } else {
                                            html! {<p>{ format!("Someone from the shadows gifted a sub to {}!", gift.broadcaster_user_name.to_string()) }</p> }
                                        }
                                    }
                                }
                            }
                        </div>
                    }
                    <div>
                    </div>
                </div>
            </>
        }
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}

pub fn listen_to_webhook(callback: Callback<String>, host: String) {
    // Spawn a background task
    let conn_string = format!("ws://{}:9000", host);
    spawn_local(async move {
        loop {
            log!("Spawning background task for webhook.");
            let ws_res = WebSocket::open(&conn_string);
            match ws_res {
                Ok(ws) => {
                    let (mut write, mut read) = ws.split();
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(data)) => {
                                log!("from websocket: {}", data);
                                callback.emit(data);
                            }
                            Ok(Message::Bytes(b)) => {
                                let decoded = std::str::from_utf8(&b);
                                if let Ok(val) = decoded {
                                    log!("from websocket: {}", val);
                                    callback.emit(val.to_string());
                                }
                            }
                            Err(e) => {
                                log!("ws: {:?}", e);
                            }
                        }
                    }
                    log!("WebSocket Closed");
                }
                Err(e) => {
                    println!("Error connecting HERE {:?}", e);
                    std::thread::sleep(std::time::Duration::from_millis(5000));
                }
            }
        }
    });
}
