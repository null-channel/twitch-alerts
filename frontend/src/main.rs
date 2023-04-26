use gloo::console::{self, Timer};
use gloo::timers::callback::{Interval, Timeout};
use yew::{html, Component, Context, Html};

pub enum Msg {
    NewEventMsg,
    EventFinished,
    PollApi,
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
        let standalone_handle =
            Interval::new(10, || console::debug!("Example of a standalone callback."));

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
                self.event_queue.push(String::from("This is a message to display to the users"));
                let link = ctx.link().clone();
                link.send_message(Msg::NewEventMsg);
                true
            }
            Msg::NewEventMsg => {
                let message = self.event_queue.pop();
                if let Some(message) = message {
                    self.current_message = message;
                }
                
                let link = ctx.link().clone();
                let timeout = Timeout::new(3000, move || link.send_message(Msg::EventFinished));
                
                Timeout::forget(timeout);
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
            // Msg::Done => {
            //     self.cancel();
            //     self.messages.push("Done!");

            //     // todo weblog
            //     // ConsoleService::group();
            //     console::info!("Done!");
            //     if let Some(timer) = self.console_timer.take() {
            //         drop(timer);
            //     }

            //     // todo weblog
            //     // ConsoleService::group_end();
            //     true
            // }
            // Msg::Tick => {
            //     self.messages.push("Tick...");
            //     // todo weblog
            //     // ConsoleService::count_named("Tick");
            //     true
            // }
            // Msg::UpdateTime => {
            //     self.time = App::get_current_time();
            //     true
            // }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let has_job = false;//self.timeout.is_some() || self.interval.is_some();
        html! {
            <>
                <div id="buttons">
                    <button disabled={has_job} onclick={ctx.link().callback(|_| Msg::NewEventMsg)}>
                        { "New Event" }
                    </button>
                    <button disabled={has_job} onclick={ctx.link().callback(|_| Msg::PollApi)}>
                        { "Poll Api" }
                    </button>
                </div>
                <div id="wrapper">
                    <div id="time">
                        { &self.time }
                    </div>
                    <div id="messages">
                        {  html! { <p>{ self.current_message.as_str() }</p> } }
                    </div>
                </div>
            </>
        }
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
