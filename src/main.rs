use failure::Error;
use futures::sync::mpsc::channel;
use log::info;
use tokio::prelude::Future;
use tokio::runtime::Runtime;

use followermaze::events::{Processor, Streamer};

fn main() -> Result<(), Error> {
    env_logger::init();

    let (tx, rx) = channel(5);
    let mut rt = Runtime::new()?;

    info!("Starting Follower Maze");

    let streamer = Streamer::new("127.0.0.1:9090", tx).expect("could not create events streamer");
    let processor =
        Processor::new("127.0.0.1:9099", rx).expect("could not create events processor");
    rt.spawn(streamer);
    rt.spawn(processor);

    //wait() returns Err(()) if Err which doesn't implement Error
    rt.shutdown_on_idle().wait().unwrap();
    Ok(())
}
