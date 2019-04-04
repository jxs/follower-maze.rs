use followermaze::{client, events};
use futures::sync::mpsc::unbounded;
use futures::sync::mpsc::UnboundedSender;
use log::info;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::prelude::Future;
use tokio::runtime::Runtime;

use events::{Processor, Streamer};

fn main() {
    env_logger::init();

    let (tx, rx) = unbounded();
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    info!("Starting Follower Maze");

    let streamer = Streamer::new("127.0.0.1:9090", tx).expect("could not create events streamer");
    rt.spawn(streamer);
    rt.spawn(client::listen("127.0.0.1:9099", clients.clone()));
    rt.spawn(Processor::new(rx, clients));

    rt.shutdown_on_idle().wait().unwrap();
}
