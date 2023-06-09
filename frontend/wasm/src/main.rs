use std::env;

use ::futures::StreamExt;
use gloo::console::{self, Timer};
use gloo::timers::callback::{Interval, Timeout};
use messages::DisplayMessage;
use reqwasm::http::Request;
use wasm_bindgen::UnwrapThrowExt;
use ws_stream_wasm::{WsMessage, WsMeta};
use yew::platform::pinned::mpsc::UnboundedSender;
use yew::platform::spawn_local;
use yew::{html, AttrValue, Callback, Component, Context, Html};

use reqwasm::websocket::{
    futures::{self, WebSocket},
    Message,
};

use pharos::*;
use wasm_bindgen::prelude::*;
use ws_stream_wasm::*;
//use futures::prelude::*;

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
    time: String,
    messages: Vec<&'static str>,
    _standalone: (Interval, Interval),
    interval: Option<Interval>,
    timeout: Option<Timeout>,
    console_timer: Option<Timer<'static>>,
    event_queue: Vec<String>,
    current_message: String,
}

impl App {
    fn get_current_time() -> String {
        let date = js_sys::Date::new_0();
        String::from(date.to_locale_time_string("en-US"))
    }

    fn cancel(&mut self) {
        self.timeout = None;
        self.interval = None;
    }
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        // create websocket connection
        let wb_callback = ctx.link().callback(Msg::WebsocketMessage);
        listen_to_webhook(wb_callback);

        // Run both futures to completion
        let standalone_handle = Interval::new(10000, || {
            console::debug!("Example of a standalone callback.")
        });

        let clock_handle = {
            let link = ctx.link().clone();
            Interval::new(30000, move || link.send_message(Msg::PollApi))
        };

        Self {
            time: App::get_current_time(),
            messages: Vec::new(),
            _standalone: (standalone_handle, clock_handle),
            interval: None,
            timeout: None,
            console_timer: None,
            event_queue: Vec::new(),
            current_message: String::from(""),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::PollApi => {
                //poll api for new events
                self.event_queue
                    .push(String::from("This is a message to display to the users"));
                let link = ctx.link().clone();
                link.send_message(Msg::NewEventMsg);
                true
            }
            Msg::NewEventMsg => {
                let message_json = self.event_queue.pop();

                if let Some(message_json) = message_json {
                    let message = serde_json::from_str::<DisplayMessage>(&message_json);

                    let Ok(message) = message else {
                        log!("Error parsing message: {}", message_json);
                        return true;
                    };

                    self.current_message = message.message;

                    log!("New message with display time: {}", message.display_time);

                    let link = ctx.link().clone();
                    let timeout = Timeout::new(message.display_time as u32, move || {
                        link.send_message(Msg::EventFinished)
                    });

                    Timeout::forget(timeout);
                }

                true
            }
            Msg::EventFinished => {
                self.current_message = String::from("");
                if !self.event_queue.is_empty() {
                    let link = ctx.link().clone();
                    link.send_message(Msg::NewEventMsg);
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
                    if self.current_message != "" {
                        <div id="clan_image">
                            <img src="img/null-logo.svg" alt="null logo"/>
                        </div>
                        <div id="messages">
                            {  html! { <p>{ self.current_message.as_str() }</p> } }
                        </div>
                    }

                </div>
            </>
        }
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}

pub fn listen_to_webhook(callback: Callback<String>) {
    // Spawn a background task that will fetch a joke and send it to the component.
    log!("Spawning background task for webhook.");

    let ws_res = WebSocket::open("ws://127.0.0.1:9000");

    match ws_res {
        Ok(ws) => {
            spawn_local(async move {
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
                            log!("ws: {:?}", e)
                        }
                    }
                }
                log!("WebSocket Closed");
            });
        }
        Err(e) => println!("Error connecting HERE {:?}", e),
    }
}
