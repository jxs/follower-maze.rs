#![feature(await_macro, async_await)]

use failure::Error;
use futures::future;
use log::info;
use tokio::sync::mpsc::channel;

use followermaze::events::{Processor, Streamer};

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let (tx, rx) = channel(5);

    info!("Starting Follower Maze");

    let streamer = Streamer::new("127.0.0.1:9090", tx).expect("could not create events streamer");
    let processor =
        Processor::new("127.0.0.1:9099", rx).expect("could not create events processor");

    future::join(streamer.run(), processor.run()).await;

    Ok(())
}
