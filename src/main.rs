use followermaze::{client, client::Client, events};
use futures::sync::mpsc::unbounded;
use log::info;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::prelude::Future;
use tokio::runtime::Runtime;

use events::Processor;

fn main() {
    env_logger::init();

    let (tx, rx) = unbounded();
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, Client>>> = Arc::new(RwLock::new(HashMap::new()));

    info!("Starting Follower Maze");

    rt.spawn(events::listen("127.0.0.1:9090", tx));
    rt.spawn(client::listen("127.0.0.1:9099", clients.clone()));
    rt.spawn(Processor::new(rx, clients));

    rt.shutdown_on_idle().wait().unwrap();
}
