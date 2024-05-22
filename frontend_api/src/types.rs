use futures_channel::mpsc::UnboundedSender;
use messages::DisplayMessage;
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio_tungstenite::tungstenite::Message;
pub type Tx = UnboundedSender<Message>;
pub type ConnectionMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;
pub type EventQueues = Arc<Mutex<Queues>>;

pub struct Queues {
    pub unpublished_events: VecDeque<DisplayMessage>,
    pub tts: VecDeque<DisplayMessage>,
    pub latest_events: VecDeque<DisplayMessage>,
    pub last_sub: Option<DisplayMessage>,
}

pub static EVENT_QUEUE_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

impl Queues {
    pub fn new() -> Queues {
        Queues {
            unpublished_events: VecDeque::new(),
            tts: VecDeque::new(),
            latest_events: VecDeque::new(),
            last_sub: None,
        }
    }
}
