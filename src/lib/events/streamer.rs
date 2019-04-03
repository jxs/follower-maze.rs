use tokio::codec::{Decoder, FramedRead, LinesCodec};
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use bytes::BytesMut;
use futures::try_ready;
use log::{debug, error};
use tokio::prelude::{Async, Future, Poll, Stream};
use tokio::net::TcpStream;
use tokio::io::ReadHalf;
use futures::sync::mpsc::UnboundedSender;

pub struct EventsDecoder {
    lines: LinesCodec,
    events_queue: HashMap<usize, Vec<String>>,
    state: usize,
}

impl EventsDecoder {
    pub fn new() -> EventsDecoder {
        EventsDecoder {
            lines: LinesCodec::new(),
            events_queue: HashMap::new(),
            state: 1,
        }
    }
}

impl Decoder for EventsDecoder {
    type Item = Vec<String>;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        let event = self.lines.decode(buf)?;
        if event.is_none() {
            return Ok(None);
        }

        let pevent: Vec<String> = event
            .as_ref()
            .unwrap()
            .trim()
            .split('|')
            .map(|x| x.to_string())
            .collect();
        let seq: usize = match pevent[0].parse() {
            Ok(seq) => seq,
            Err(err) => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("events listener could not parse event, {}", err),
                ));
            }
        };

        self.events_queue.insert(seq, pevent.clone());
        if let Some(pevent) = self.events_queue.remove(&self.state) {
            self.state += 1;
            return Ok(Some(pevent));
        }
        Ok(None)
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        //process remaining events in buffer
        while !buf.is_empty() {
            if let Some(event) = self.decode(buf)? {
                return Ok(Some(event));
            }
        }

        if let Some(pevent) = self.events_queue.remove(&self.state) {
            self.state += 1;
            return Ok(Some(pevent));
        }
        Ok(None)
    }
}

pub struct Streamer {
    reader: FramedRead<ReadHalf<TcpStream>, EventsDecoder>,
    tx: UnboundedSender<Vec<String>>
}

impl Streamer {
    pub fn new(reader: FramedRead<ReadHalf<TcpStream>, EventsDecoder>, tx: UnboundedSender<Vec<String>>) -> Streamer {
        Streamer{reader, tx}
    }
}

impl Future for Streamer {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            let event = match self.reader.poll() {
                Err(err) => {
                    error!("events streamer frame read error {:?}", err);
                    panic!();
                }
                Ok(result) => match try_ready!(Ok(result)) {
                    Some(event) => event,
                    None => return Ok(Async::Ready(())),
                }
            };

            self.tx.unbounded_send(event.clone()).unwrap_or_else(|err| {
                error!(
                    "events listener error sending event: {} : {}",
                    event.join("|"),
                    err
                );
                panic!()
            });
            debug!("events listener sent event : {}", event.join("|"),);
        }
    }
}
