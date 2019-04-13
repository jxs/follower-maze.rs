use bytes::BytesMut;
use failure::Error;
use futures::sink::Send;
use futures::sync::mpsc::Sender;
use futures::try_ready;
use log::debug;
use std::collections::HashMap;
use std::default::Default;
use std::io::{Error as IoError, ErrorKind};
use std::string::ToString;
use tokio::codec::{Decoder, FramedRead, LinesCodec};
use tokio::io::{AsyncRead, ReadHalf};
use tokio::net::{tcp::Incoming, TcpListener, TcpStream};
use tokio::prelude::{Async, Future, Poll, Sink, Stream};

pub struct EventsDecoder {
    lines: LinesCodec,
    events_queue: HashMap<usize, Vec<String>>,
    state: usize,
}

impl Default for EventsDecoder {
    fn default() -> Self {
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
        let pevent: Vec<String> = match self.lines.decode(buf)? {
            Some(pevent) => pevent.trim().split('|').map(ToString::to_string).collect(),
            None => return Ok(None),
        };

        let seq: usize = match pevent[0].parse() {
            Ok(seq) => seq,
            Err(err) => {
                return Err(IoError::new(
                    ErrorKind::InvalidInput,
                    format!("events listener could not parse event, {}", err),
                )
                .into());
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

enum State {
    //waiting for tcp connection
    Connecting(Incoming),
    //waiting for events on socket,
    Waiting,
    //streaming event to channel
    Streaming(Send<Sender<Vec<String>>>, Vec<String>),
}

pub struct Streamer {
    tx: Option<Sender<Vec<String>>>,
    socket: Option<FramedRead<ReadHalf<TcpStream>, EventsDecoder>>,
    state: State,
}

impl Streamer {
    pub fn new(addr: &str, tx: Sender<Vec<String>>) -> Result<Streamer, Error> {
        let addr = addr.parse()?;
        let connect_future = TcpListener::bind(&addr)?.incoming();
        Ok(Streamer {
            tx: Some(tx),
            socket: None,
            state: State::Connecting(connect_future),
        })
    }
}

impl Future for Streamer {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            match &mut self.state {
                State::Connecting(f) => match f.poll().expect("events streamer error") {
                    Async::NotReady => return Ok(Async::NotReady),
                    Async::Ready(Some(socket)) => {
                        self.socket =
                            Some(FramedRead::new(socket.split().0, EventsDecoder::default()));
                        self.state = State::Waiting;
                    }
                    Async::Ready(None) => unreachable!(),
                },
                State::Waiting => {
                    // when the state is connecting we know we have the socket
                    let result = self
                        .socket
                        .as_mut()
                        .expect("Attempted to poll Streamer after completion")
                        .poll()
                        .expect("events streamer frame read error");

                    match result {
                        Async::NotReady => return Ok(Async::NotReady),
                        Async::Ready(Some(event)) => {
                            self.state = State::Streaming(
                                //Streaming state is only entered when there's an event read from the socket
                                //on that ocasion tx is always Some, Tx only becomes None while on the Streaming state,
                                //after finishing streaming the sink is returned and put back on tx
                                self.tx.take().unwrap().send(event.clone()),
                                event.clone(),
                            );
                            debug!("events listener sent event : {}", event.join("|"),);
                        }
                        Async::Ready(None) => return Ok(Async::Ready(())),
                    }
                }
                State::Streaming(sender, event) => {
                    let event_ok = sender.poll().unwrap_or_else(|err| {
                        panic!(
                            "events listener error sending event: {}, {}",
                            event.join("|"),
                            err
                        )
                    });
                    let tx = try_ready!(Ok(event_ok));
                    self.tx = Some(tx);
                    self.state = State::Waiting;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EventsDecoder;
    use bytes::BytesMut;
    use tokio::codec::Decoder;

    #[test]
    fn decoder_sorts_events_by_order() {
        let event_seq = "4|S|32\n1|B\n3|P|32|56\n2|U|12|9\n";
        let mut buf = BytesMut::from(event_seq);
        let mut decoder = EventsDecoder::default();
        assert_eq!(
            vec!["1".to_string(), "B".to_string()],
            decoder.decode_eof(&mut buf).unwrap().unwrap()
        );
        assert_eq!(
            vec![
                "2".to_string(),
                "U".to_string(),
                "12".to_string(),
                "9".to_string()
            ],
            decoder.decode_eof(&mut buf).unwrap().unwrap()
        );
        assert_eq!(
            vec![
                "3".to_string(),
                "P".to_string(),
                "32".to_string(),
                "56".to_string()
            ],
            decoder.decode_eof(&mut buf).unwrap().unwrap()
        );
        assert_eq!(
            vec!["4".to_string(), "S".to_string(), "32".to_string()],
            decoder.decode_eof(&mut buf).unwrap().unwrap()
        );
        assert_eq!(None, decoder.decode_eof(&mut buf).unwrap());
    }
}
