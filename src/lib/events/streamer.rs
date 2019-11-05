use bytes::BytesMut;
use anyhow::Error;
use futures::StreamExt;
use log::debug;
use std::collections::HashMap;
use std::default::Default;
use std::io::{Error as IoError, ErrorKind};
use std::string::ToString;
use tokio::codec::{Decoder, FramedRead, LinesCodec};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;

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

pub struct Streamer {
    tx: Sender<Vec<String>>,
    socket: TcpListener,
}

impl Streamer {
    pub async fn new(addr: &str, tx: Sender<Vec<String>>) -> Result<Streamer, Error> {
        let socket = TcpListener::bind(&addr).await?;
        Ok(Streamer { tx: tx, socket })
    }

    pub async fn run(mut self) {
        let mut incoming = self.socket.incoming();
        let mut reader = match incoming.next().await {
            Some(Ok(reader)) => FramedRead::new(reader, EventsDecoder::default()),
            Some(Err(err)) => panic!("error reading streamer socket, {}", err),
            None => unreachable!(),
        };
        debug!("events streamer socket connect!");
        loop {
            if let Some(Ok(event)) = reader.next().await {
                if let Err(err) = self.tx.send(event.clone()).await {
                    panic!("error reading events from streamer socket, {}", err);
                }
                debug!("send event {} to processor!", event.join("|"));
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
