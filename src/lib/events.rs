use crate::client::Client;
use bytes::BytesMut;
use futures::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use log::{debug, error, info, trace};
use std::collections::{HashMap, HashSet};
use std::io::{Error, ErrorKind};
use std::sync::{Arc, RwLock};
use tokio;
use tokio::codec::{Decoder, FramedRead, LinesCodec};
use tokio::io::AsyncRead;
use tokio::net::TcpListener;
use tokio::prelude::{Future, Stream};

struct EventsDecoder {
    lines: LinesCodec,
    events_queue: HashMap<usize, Vec<String>>,
    state: usize,
}

impl EventsDecoder {
    fn new() -> EventsDecoder {
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

    fevents_source.and_then(|events_source| {
        events_source
            .for_each(move |event| {
                tx.unbounded_send(event.clone()).unwrap_or_else(|err| {
                    error!(
                        "events listener error sending event: {} : {}",
                        event.join("|"),
                        err
                    );
                    panic!()
                });

                debug!("events listener sent event : {}", event.join("|"),);
                Ok(())
            })
            .map_err(|err| {
                error!("events listener error {:?}", err);
            })
    })
}

pub fn handle<S: ::std::hash::BuildHasher>(
    tx: UnboundedReceiver<Vec<String>>,
    clients: Arc<RwLock<HashMap<String, Client, S>>>,
) -> impl Future<Item = (), Error = ()> {
    let followers: Arc<RwLock<HashMap<String, HashSet<String>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    info!("events handler, waiting for events");
    tx.for_each(move |event| {
        debug!("Received event! {:?}", event);
        let mut clients = clients.write().unwrap();
        let event_str = event.join("|");
        match event[1].as_str() {
            "P" => {
                let client_id = event[3].clone();
                match clients.get_mut(&client_id) {
                    Some(client) => {
                        client.send(event.clone());
                    }
                    _ => debug!(
                        "events handler skipping event {}, client {} not found",
                        event_str, client_id
                    ),
                }
            }
            "F" => {
                let client_id = event[3].clone();
                match clients.get_mut(&client_id) {
                    Some(client) => {
                        let mut followers = followers.write().unwrap();
                        let followers = followers.entry(client_id).or_insert_with(HashSet::new);
                        followers.insert(event[2].clone());
                        client.send(event.clone());
                    }
                    _ => {
                        let mut followers = followers.write().unwrap();
                        let followers = followers
                            .entry(client_id.clone())
                            .or_insert_with(HashSet::new);
                        followers.insert(event[2].clone());

                        debug!(
                            "events handler skipping event {}, client {} not found, but adding to its follower list",
                            event_str, client_id
                        )
                    }
                }
            }
            "U" => {
                let client_id = event[3].clone();
                let unfollower_id = &event[2].clone();
                let mut followers = followers.write().unwrap();
                match followers.get_mut(&client_id) {
                    Some(followers) => {
                        followers.retain(|follower_id| follower_id != unfollower_id);
                    }
                    None => debug!(
                        "events handler skipping unfollow, client: {} isn't followed by {}",
                        client_id, unfollower_id
                    ),
                }
            }
            "B" => {
                for (_client_id, client) in clients.iter_mut() {
                    client.send(event.clone());
                }
            }
            "S" => {
                let client_id = event[2].clone();

                let followers = followers.read().unwrap();

                let followers = match followers.get(&client_id) {
                    Some(followers) => followers,
                    None => {
                        debug!(
                            "events handler skipping sending event {}, client:{} not found",
                            event_str, client_id
                        );
                        return Ok(());
                    }
                };

                trace!(
                    "events handler client: {} followers: {:?}",
                    client_id,
                    followers
                );
                for follower_id in followers.iter() {
                    match clients.get_mut(follower_id) {
                        Some(follower) => follower.send(event.clone()),
                        None => {
                            debug!(
                                "events handler skipping sending event {}, follower:{} not found",
                                event_str, follower_id
                            );
                            continue;
                        }
                    };
                }
            }
            _ => (),
        }
        Ok(())
    })
}
