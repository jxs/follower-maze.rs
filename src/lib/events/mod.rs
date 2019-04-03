pub mod processor;
pub mod streamer;

use futures::sync::mpsc::UnboundedSender;
use log::{info, error};
use tokio;
use tokio::codec::FramedRead;
use tokio::io::AsyncRead;
use tokio::prelude::{Future, Stream};
use tokio::net::TcpListener;

pub use processor::Processor;
pub use streamer::{Streamer, EventsDecoder};

pub fn listen(addr: &str, tx: UnboundedSender<Vec<String>>) -> impl Future<Item = (), Error = ()> {
    let addr = addr.parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    info!("events listener Listening for events source on {}", addr);
    let fevents_source = listener
        .incoming()
        .take(1)
        .collect()
        .map(|mut v| v.pop().unwrap())
        .map(|socket| FramedRead::new(socket.split().0, EventsDecoder::new()))
        .map_err(|err| {
            error!("events listener frame read error {:?}", err);
        });

    fevents_source.and_then(|framed| {
        Streamer::new(framed, tx)
    })
}
