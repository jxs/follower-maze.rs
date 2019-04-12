use failure::Error;
use futures::sync::mpsc::channel;
use log::info;
use tokio::prelude::future::lazy;

use followermaze::events::{Processor, Streamer};

fn main() -> Result<(), Error> {
    env_logger::init();

    let (tx, rx) = channel(5);

    info!("Starting Follower Maze");

    let streamer = Streamer::new("127.0.0.1:9090", tx).expect("could not create events streamer");
    let processor =
        Processor::new("127.0.0.1:9099", rx).expect("could not create events processor");

    tokio::run(lazy(move || {
        tokio::spawn(streamer);
        tokio::spawn(processor);
        Ok(())
    }));

    Ok(())
}
