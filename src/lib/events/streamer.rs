use bytes::BytesMut;
use futures::sync::mpsc::UnboundedSender;
use futures::try_ready;
use log::{debug, error};
use std::collections::HashMap;
use std::default::Default;
use std::io::{Error, ErrorKind};
use tokio::codec::{Decoder, FramedRead, LinesCodec};
use tokio::io::{AsyncRead, ReadHalf};
use tokio::net::{tcp::Incoming, TcpListener, TcpStream};
use tokio::prelude::{Async, Future, Poll, Stream};

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
        let event = self.lines.decode(buf)?;
        if event.is_none() {
            return Ok(None);
        }

        let pevent: Vec<String> = event
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

enum State {
    //waiting for tcp connection
    Connecting(Incoming),
    //streaming events,
    Streaming(FramedRead<ReadHalf<TcpStream>, EventsDecoder>),
}

pub struct Streamer {
    tx: UnboundedSender<Vec<String>>,
    state: State,
}

impl Streamer {
    pub fn new(addr: &str, tx: UnboundedSender<Vec<String>>) -> Result<Streamer, Error> {
        let addr = addr.parse().unwrap();
        let connect_future = TcpListener::bind(&addr)?.incoming();
        Ok(Streamer {
            tx,
            state: State::Connecting(connect_future),
        })
    }
}

impl Future for Streamer {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            match self.state {
                State::Connecting(ref mut f) => {
                    let result = f.poll();
                    if let Err(err) = result {
                        error!("events streamer error: {}", err);
                        panic!();
                    }

                    match try_ready!(Ok(result.unwrap())) {
                        Some(socket) => {
                            let reader = FramedRead::new(socket.split().0, EventsDecoder::default());
                            self.state = State::Streaming(reader);
                        }
                        None => unreachable!(),
                    }
                }
                State::Streaming(ref mut reader) => {
                    let result = reader.poll();

                    if let Err(err) = result {
                        error!("events streamer frame read error {:?}", err);
                        panic!();
                    }

                    match try_ready!(Ok(result.unwrap())) {
                        Some(event) => {
                            if let Err(err) = self.tx.unbounded_send(event.clone()) {
                                error!(
                                    "events listener error sending event: {} : {}",
                                    event.join("|"),
                                    err
                                );
                                panic!()
                            }
                            debug!("events listener sent event : {}", event.join("|"),);
                        }
                        None => return Ok(Async::Ready(())),
                    }
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
