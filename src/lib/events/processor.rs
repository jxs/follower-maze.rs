use futures::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::try_ready;
use log::{debug, error, trace};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::prelude::{Async, Future, Poll, Stream};

pub struct Processor<S: ::std::hash::BuildHasher> {
    followers: HashMap<String, HashSet<String>>,
    events_stream: UnboundedReceiver<Vec<String>>,
    clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>, S>>>,
}

impl<S: ::std::hash::BuildHasher> Future for Processor<S> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            match try_ready!(self.events_stream.poll()) {
                Some(event) => {
                    self.process_event(event);
                }
                None => return Ok(Async::Ready(())),
            }
        }
    }
}

impl<S: ::std::hash::BuildHasher> Processor<S> {
    pub fn new(
        events_stream: UnboundedReceiver<Vec<String>>,
        clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>, S>>>,
    ) -> Processor<S> {
        Processor {
            followers: HashMap::new(),
            events_stream,
            clients,
        }
    }

    fn send_event(
        &self,
        client_id: String,
        client: &UnboundedSender<Vec<String>>,
        event: Vec<String>,
    ) {
        if let Err(err) = client.unbounded_send(event.clone()) {
            error!(
                "error sending event {}, to client {}, {}",
                event.join("|"),
                client_id,
                err
            );
        }
        debug!("send event {} to client {}", event.join("|"), client_id);
    }

    fn process_event(&mut self, event: Vec<String>) {
        debug!("Received event! {:?}", event);
        let mut clients = self.clients.write().unwrap();
        let event_str = event.join("|");
        match event[1].as_str() {
            "P" => {
                let client_id = event[3].clone();
                match clients.get_mut(&client_id) {
                    Some(client) => {
                        self.send_event(client_id, client, event);
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
                        let followers = self
                            .followers
                            .entry(client_id.clone())
                            .or_insert_with(HashSet::new);
                        followers.insert(event[2].clone());
                        self.send_event(client_id, client, event);
                    }
                    _ => {
                        let followers = self
                            .followers
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

                match self.followers.get_mut(&client_id) {
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
                for (client_id, client) in clients.iter_mut() {
                    self.send_event(client_id.to_string(), client, event.clone());
                }
            }
            "S" => {
                let client_id = event[2].clone();

                let followers = match self.followers.get(&client_id) {
                    Some(followers) => followers,
                    None => {
                        debug!(
                            "events handler skipping sending event {}, client:{} not found",
                            event_str, client_id
                        );
                        return;
                    }
                };

                trace!(
                    "events handler client: {} followers: {:?}",
                    client_id,
                    followers
                );
                for follower_id in followers.iter() {
                    match clients.get_mut(follower_id) {
                        Some(follower) => {
                            self.send_event(follower_id.to_string(), follower, event.clone())
                        }
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
            _ => {}
        }
    }
}
