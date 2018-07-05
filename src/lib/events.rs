use client::Client;
use futures::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::sync::{Arc, RwLock};
use tokio;
use tokio::net::TcpListener;
use tokio::prelude::{
    future::{loop_fn, Loop}, Future, Stream,
};

pub fn listen(addr: &str, tx: UnboundedSender<Vec<String>>) -> impl Future<Item = (), Error = ()> {
    let addr = addr.parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    info!("events listener Listening for events source on {}", addr);
    listener
        .incoming()
        .for_each(move |socket| {
            info!("events listener connected");
            let event_stream_loop = loop_fn(
                (1, HashMap::new(), tx.clone(), BufReader::new(socket)),
                |(mut state, mut events_queue, tx, reader)| {
                    tokio::io::read_until(reader, b'\n', Vec::new())
                        .and_then(move |(reader, raw_event)| {
                            let event_str = match String::from_utf8(raw_event) {
                                Ok(string) => string,
                                Err(err) => {
                                    error!("events listener could not parse event, {}", err);
                                    return Ok(Loop::Continue((state, events_queue, tx, reader)));
                                }
                            };
                            if event_str.is_empty() {
                                return Ok(Loop::Break(()));
                            }
                            debug!("events listener read event: {}", event_str);
                            let event: Vec<String> =
                                event_str.trim().split('|').map(|x| x.to_string()).collect();
                            let seq: usize = match event[0].parse() {
                                Ok(seq) => seq,
                                Err(err) => {
                                    error!("events listener could not parse event, {}", err);
                                    return Ok(Loop::Continue((state, events_queue, tx, reader)));
                                }
                            };
                            trace!("events listener inserted event: {} -- {:?}", seq, event);
                            events_queue.insert(seq, event);
                            loop {
                                match events_queue.remove(&state) {
                                    Some(event) => {
                                        tx.unbounded_send(event.clone()).unwrap_or_else(|err| {
                                            error!(
                                                "events listener error sending event: {} : {}",
                                                event_str, err
                                            );
                                            panic!()
                                        });
                                        state += 1;

                                        debug!(
                                            "events listener sent event : {}, state: {}",
                                            event.join("|"),
                                            state
                                        );
                                    }
                                    None => {
                                        break;
                                    }
                                };
                            }

                            Ok(Loop::Continue((state, events_queue, tx, reader)))
                        })
                        .map_err(|err| {
                            error!("events listener error {:?}", err);
                        })
                },
            );
            tokio::spawn(event_stream_loop);
            Ok(())
        })
        .map_err(|err| {
            error!("events listener error {:?}", err);
        })
}

pub fn handle(
    tx: UnboundedReceiver<Vec<String>>,
    clients: Arc<RwLock<HashMap<String, Client>>>,
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
                        let followers = followers.entry(client_id).or_insert(HashSet::new());
                        followers.insert(event[2].clone());
                        client.send(event.clone());
                    }
                    _ => {
                        let mut followers = followers.write().unwrap();
                        let followers =
                            followers.entry(client_id.clone()).or_insert(HashSet::new());
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
