use futures::sync::mpsc::unbounded;
use log::info;
use tokio::prelude::Future;
use tokio::runtime::Runtime;

use followermaze::events::{Processor, Streamer};

fn main() {
    env_logger::init();

    let (tx, rx) = unbounded();
    let mut rt = Runtime::new().unwrap();

    info!("Starting Follower Maze");

    let streamer = Streamer::new("127.0.0.1:9090", tx).expect("could not create events streamer");
    let processor =
        Processor::new("127.0.0.1:9099", rx).expect("could not create events processor");
    rt.spawn(streamer);
    rt.spawn(processor);

    rt.shutdown_on_idle().wait().unwrap();
}
