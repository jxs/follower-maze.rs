#![feature(await_macro, async_await, futures_api)]

use failure::Error;
use futures::channel::mpsc::channel;
use futures::future;
use log::info;

use followermaze::events::{Processor, Streamer};

#[runtime::main(runtime_tokio::Tokio)]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let (tx, rx) = channel(5);

    info!("Starting Follower Maze");

    let streamer = Streamer::new("127.0.0.1:9090", tx).expect("could not create events streamer");
    let processor =
        Processor::new("127.0.0.1:9099", rx).expect("could not create events processor");

    await!(future::join(streamer.run(), processor.run()));

    Ok(())
}
