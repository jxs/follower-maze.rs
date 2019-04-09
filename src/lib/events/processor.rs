use failure::Error;
use futures::sync::mpsc::{unbounded, Receiver, UnboundedSender};
use log::{debug, error, info, trace};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::io::{AsyncRead, ReadHalf, WriteHalf};
use tokio::net::{tcp::Incoming, TcpListener, TcpStream};
use tokio::prelude::{Async, Future, Poll, Stream};

use crate::client::Client;

enum ListenerState {
    //listening for client connections
    Listening,
    //connecting client, reading it's ID
    Connecting(
        WriteHalf<TcpStream>,
        FramedRead<ReadHalf<TcpStream>, LinesCodec>,
    ),
    //when polling the listener, used to move the state while borrowed
    Empty,
}

struct Listener {
    inner: Incoming,
    state: ListenerState,
}

impl Stream for Listener {
    type Item = (String, WriteHalf<TcpStream>);
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            let listener_state = std::mem::replace(&mut self.state, ListenerState::Empty);
            match listener_state {
                ListenerState::Listening => match self.inner.poll() {
                    Err(err) => {
                        self.state = ListenerState::Listening;
                        return Err(err.into());
                    }
                    Ok(socket_ok) => match socket_ok {
                        Async::NotReady => {
                            self.state = ListenerState::Listening;
                            return Ok(Async::NotReady);
                        }
                        Async::Ready(option) => match option {
                            Some(socket) => {
                                let (reader, writer) = socket.split();
                                let framed = FramedRead::new(reader, LinesCodec::new());
                                debug!("clients listener client connected");
                                self.state = ListenerState::Connecting(writer, framed)
                            }
                            None => unreachable!(),
                        },
                    },
                },

                ListenerState::Connecting(writer, mut framed) => match framed.poll() {
                    Err(err) => return Err(err.into()),
                    Ok(result) => match result {
                        Async::NotReady => {
                            self.state = ListenerState::Connecting(writer, framed);
                            return Ok(Async::NotReady);
                        }
                        Async::Ready(item) => match item {
                            Some(id) => {
                                debug!("clients listener client read client id: {:?}", id);
                                self.state = ListenerState::Listening;
                                return Ok(Async::Ready(Some((id, writer))));
                            }
                            None => unreachable!(),
                        },
                    },
                },
                ListenerState::Empty => unreachable!(),
            }
        }
    }
}

impl Listener {
    fn new(addr: &SocketAddr) -> Result<Listener, Error> {
        let inner = TcpListener::bind(addr)?;

        info!(
            "clients listener Listening for clients on {}",
            inner.local_addr()?
        );

        Ok(Listener {
            inner: inner.incoming(),
            state: ListenerState::Listening,
        })
    }
}

pub struct Processor {
    followers: HashMap<String, HashSet<String>>,
    events_stream: Receiver<Vec<String>>,
    clients: HashMap<String, UnboundedSender<Vec<String>>>,
    listener: Listener,
}

impl Future for Processor {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            let mut listener_state = self.listener.poll().expect("error listening for clients!");

            if let Async::Ready(option) = listener_state {
                if let Some((id, socket)) = option {
                    let (tx, rx) = unbounded();
                    let client = Client::new(id.clone(), socket, rx);
                    tokio::spawn(client);
                    self.clients.insert(id, tx);
                }
                // required because option was moved
                listener_state = Async::Ready(None);
            }

            match self.events_stream.poll() {
                Err(_err) => unreachable!(),
                Ok(result) => match result {
                    Async::NotReady => {
                        if listener_state.is_not_ready() {
                            return Ok(Async::NotReady);
                        }
                    }
                    Async::Ready(result) => match result {
                        Some(event) => {
                            self.process_event(event);
                        }
                        None => return Ok(Async::Ready(())),
                    },
                },
            }
        }
    }
}

impl Processor {
    pub fn new(addr: &str, events_stream: Receiver<Vec<String>>) -> Result<Processor, Error> {
        let addr = addr.parse()?;
        Ok(Processor {
            followers: HashMap::new(),
            events_stream,
            clients: HashMap::new(),
            listener: Listener::new(&addr)?,
        })
    }

    //send the event to the client via channel
    fn send_event(client_id: String, client: &UnboundedSender<Vec<String>>, event: Vec<String>) {
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

    //process the event by type and send it to the matching clients
    fn process_event(&mut self, event: Vec<String>) {
        debug!("Received event! {:?}", event);
        let event_str = event.join("|");
        match event[1].as_str() {
            "P" => {
                let client_id = event[3].clone();
                match self.clients.get_mut(&client_id) {
                    Some(client) => {
                        Self::send_event(client_id, client, event);
                    }
                    _ => debug!(
                        "events handler skipping event {}, client {} not found",
                        event_str, client_id
                    ),
                }
            }
            "F" => {
                let client_id = event[3].clone();
                match self.clients.get_mut(&client_id) {
                    Some(client) => {
                        let followers = self
                            .followers
                            .entry(client_id.clone())
                            .or_insert_with(HashSet::new);
                        followers.insert(event[2].clone());
                        Self::send_event(client_id, client, event);
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
                for (client_id, client) in self.clients.iter_mut() {
                    Self::send_event(client_id.to_string(), client, event.clone());
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
                    match self.clients.get_mut(follower_id) {
                        Some(follower) => {
                            Self::send_event(follower_id.to_string(), follower, event.clone())
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

#[cfg(test)]
mod tests {
    use super::{Listener, ListenerState, Processor};
    use futures::sync::mpsc::{channel, unbounded, Receiver, UnboundedReceiver, UnboundedSender};
    use std::collections::HashMap;
    use std::io::Write;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::prelude::{Future, Stream};

    fn seed_clients() -> (
        HashMap<String, UnboundedSender<Vec<String>>>,
        HashMap<String, UnboundedReceiver<Vec<String>>>,
    ) {
        let mut clients = HashMap::new();
        let mut rxs = HashMap::new();

        let (tx, rx) = unbounded();
        clients.insert("354".to_string(), tx);
        rxs.insert("354".to_string(), rx);

        let (tx, rx) = unbounded();
        clients.insert("274".to_string(), tx);
        rxs.insert("274".to_string(), rx);

        let (tx, rx) = unbounded();
        clients.insert("184".to_string(), tx);
        rxs.insert("184".to_string(), rx);

        let (tx, rx) = unbounded();
        clients.insert("134".to_string(), tx);
        rxs.insert("134".to_string(), rx);
        (clients, rxs)
    }

    #[test]
    fn listener_reads_client_id() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let tcp_listener = TcpListener::bind(&addr).unwrap();
        let local_addr = tcp_listener.local_addr().unwrap();
        let listener = Listener {
            inner: tcp_listener.incoming(),
            state: ListenerState::Listening,
        };

        let mut client = TcpStream::connect(&local_addr).wait().unwrap();
        client.write(b"1415\n").unwrap();
        let fut = listener.take(1).collect().wait().unwrap();
        let (client_id, _) = &fut[0];
        assert_eq!("1415", client_id);
    }

    #[test]
    fn processor_sends_broadcast_event_to_all_clients() {
        let addr = "127.0.0.1:0".parse().unwrap();

        let (_tx, rx) = channel(5);
        let (txs, rxs) = seed_clients();
        let mut processor = Processor {
            followers: HashMap::new(),
            events_stream: rx,
            clients: txs,
            listener: Listener::new(&addr).unwrap(),
        };
        let event = vec!["342".to_string(), "B".to_string()];
        processor.process_event(event.clone());
        for (_client_id, client) in rxs.into_iter() {
            let received_event = client.take(1).collect().wait().unwrap();
            assert_eq!(event, received_event[0]);
        }
    }

    #[test]
    fn processor_sends_private_message_to_matching_client() {
        let addr = "127.0.0.1:0".parse().unwrap();

        let (_tx, rx) = channel(5);
        let (txs, mut rxs) = seed_clients();
        let mut processor = Processor {
            followers: HashMap::new(),
            events_stream: rx,
            clients: txs,
            listener: Listener::new(&addr).unwrap(),
        };
        let event = vec![
            "34".to_string(),
            "P".to_string(),
            "354".to_string(),
            "274".to_string(),
        ];
        processor.process_event(event.clone());
        let client = rxs.remove("274").unwrap();
        let received_event = client.take(1).collect().wait().unwrap();
        assert_eq!(event, received_event[0]);
    }

    #[test]
    fn processor_sends_status_update_message_to_matching_client_after_follow() {
        let addr = "127.0.0.1:0".parse().unwrap();

        let (_tx, rx) = channel(5);
        let (txs, mut rxs) = seed_clients();
        let mut processor = Processor {
            followers: HashMap::new(),
            events_stream: rx,
            clients: txs,
            listener: Listener::new(&addr).unwrap(),
        };
        processor.process_event(vec![
            "15".to_string(),
            "F".to_string(),
            "134".to_string(),
            "184".to_string(),
        ]);
        let event = vec!["18".to_string(), "S".to_string(), "184".to_string()];
        processor.process_event(event.clone());
        let client = rxs.remove("134").unwrap();
        let received_event = client.take(1).collect().wait().unwrap();
        assert_eq!(event, received_event[0]);
    }

    #[test]
    fn processor_doesnt_send_status_update_message_to_matching_client_after_unfollow() {
        let addr = "127.0.0.1:0".parse().unwrap();

        let (_tx, rx) = channel(5);
        let (txs, mut rxs) = seed_clients();
        let mut processor = Processor {
            followers: HashMap::new(),
            events_stream: rx,
            clients: txs,
            listener: Listener::new(&addr).unwrap(),
        };

        processor.process_event(vec![
            "15".to_string(),
            "F".to_string(),
            "354".to_string(),
            "184".to_string(),
        ]);

        let event1 = vec!["18".to_string(), "S".to_string(), "184".to_string()];
        processor.process_event(event1.clone());

        processor.process_event(vec![
            "25".to_string(),
            "U".to_string(),
            "354".to_string(),
            "184".to_string(),
        ]);

        processor.process_event(vec!["28".to_string(), "S".to_string(), "184".to_string()]);
        let event2 = vec!["30".to_string(), "B".to_string()];
        processor.process_event(event2.clone());

        let client = rxs.remove("354").unwrap();
        let mut received_events = client.take(2).collect().wait().unwrap().into_iter();
        let received_event1 = received_events.next().unwrap();

        assert_eq!(event1, received_event1);

        let received_event2 = received_events.next().unwrap();
        assert_eq!(event2, received_event2);
    }
}
