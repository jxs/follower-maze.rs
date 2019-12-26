use crate::client::Client;
use anyhow::Error;
use futures::{select, FutureExt, StreamExt};
use log::{debug, error, info, trace};
use std::collections::{HashMap, HashSet};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{unbounded_channel, Receiver, UnboundedSender};
use tokio_util::codec::{FramedRead, LinesCodec};

pub struct Processor {
    followers: HashMap<String, HashSet<String>>,
    events_stream: Receiver<Vec<String>>,
    clients: HashMap<String, UnboundedSender<Vec<String>>>,
    listener: Option<TcpListener>,
}

impl Processor {
    pub async fn new(addr: &str, events_stream: Receiver<Vec<String>>) -> Result<Processor, Error> {
        Ok(Processor {
            followers: HashMap::new(),
            events_stream,
            clients: HashMap::new(),
            listener: Some(TcpListener::bind(&addr).await?),
        })
    }

    pub async fn run(mut self) {
        let mut listener = self.listener.take().expect("run can only be polled once");
        info!("clients listener Listening for clients on",);
        let mut incoming = listener.incoming();

        loop {
            select!(
                mut client_socket_f = incoming.next().fuse() => match client_socket_f {
                    Some(Ok(mut client_socket)) => {
                        let (reader, writer) = client_socket.split();
                        let mut lines = FramedRead::new(reader, LinesCodec::new());
                        let id = match lines.next().await {
                            Some(Ok(id)) => id,
                            Some(Err(err)) => panic!("error reading id from client socket, {}", err),
                            None => panic!("error reading id from client socket, disconected early"),
                        };
                        let (tx, rx) = unbounded_channel();
                        let client = Client::new(id.clone(), client_socket, rx);
                        tokio::spawn(client.run());
                        self.clients.insert(id.clone(), tx);
                        debug!("processor inserted client {}", id);
                    },
                    Some(Err(err)) => panic!("error reading client from socket, {}", err),
                    None => unreachable!()
                },
                event = self.events_stream.next().fuse() => match event {
                    Some(event) => {
                        self.process_event(event).await
                    },
                    None => return
                },
            );
        }
    }

    //send the event to the client via channel
    async fn send_event(
        client_id: String,
        client: &mut UnboundedSender<Vec<String>>,
        event: Vec<String>,
    ) {
        if let Err(err) = client.send(event.clone()) {
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
    async fn process_event(&mut self, event: Vec<String>) {
        debug!("Received event! {:?}", event);
        let event_str = event.join("|");
        match event[1].as_str() {
            "P" => {
                let client_id = event[3].clone();
                match self.clients.get_mut(&client_id) {
                    Some(ref mut client) => {
                        Self::send_event(client_id, client, event).await;
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
                        Self::send_event(client_id, client, event).await;
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
                    Self::send_event(client_id.to_string(), client, event.clone()).await;
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
                            Self::send_event(follower_id.to_string(), follower, event.clone()).await
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
    use super::Processor;
    use futures::executor::block_on;
    use futures::StreamExt;
    use std::collections::HashMap;
    use tokio::sync::mpsc::{channel, unbounded_channel, UnboundedReceiver, UnboundedSender};

    fn seed_clients() -> (
        HashMap<String, UnboundedSender<Vec<String>>>,
        HashMap<String, UnboundedReceiver<Vec<String>>>,
    ) {
        let mut clients = HashMap::new();
        let mut rxs = HashMap::new();

        let (tx, rx) = unbounded_channel();
        clients.insert("354".to_string(), tx);
        rxs.insert("354".to_string(), rx);

        let (tx, rx) = unbounded_channel();
        clients.insert("274".to_string(), tx);
        rxs.insert("274".to_string(), rx);

        let (tx, rx) = unbounded_channel();
        clients.insert("184".to_string(), tx);
        rxs.insert("184".to_string(), rx);

        let (tx, rx) = unbounded_channel();
        clients.insert("134".to_string(), tx);
        rxs.insert("134".to_string(), rx);
        (clients, rxs)
    }

    #[tokio::test]
    async fn processor_sends_broadcast_event_to_all_clients() {
        let (_tx, rx) = channel(5);
        let (txs, rxs) = seed_clients();
        let mut processor = Processor::new("127.0.0.1:0", rx).await.unwrap();
        processor.clients = txs;
        let event = vec!["342".to_string(), "B".to_string()];
        processor.process_event(event.clone()).await;
        for (_client_id, mut client) in rxs.into_iter() {
            let received_event = block_on(client.next());
            assert_eq!(event, received_event.unwrap());
        }
    }

    #[tokio::test]
    async fn processor_sends_private_message_to_matching_client() {
        let (_tx, rx) = channel(5);
        let (txs, mut rxs) = seed_clients();
        let mut processor = Processor::new("127.0.0.1:0", rx).await.unwrap();
        processor.clients = txs;

        let event = vec![
            "34".to_string(),
            "P".to_string(),
            "354".to_string(),
            "274".to_string(),
        ];
        processor.process_event(event.clone()).await;
        let mut client = rxs.remove("274").unwrap();
        let received_event = block_on(client.next());
        assert_eq!(event, received_event.unwrap());
    }

    #[tokio::test]
    async fn processor_sends_status_update_message_to_matching_client_after_follow() {
        let (_tx, rx) = channel(5);
        let (txs, mut rxs) = seed_clients();
        let mut processor = Processor::new("127.0.0.1:0", rx).await.unwrap();
        processor.clients = txs;

        processor
            .process_event(vec![
                "15".to_string(),
                "F".to_string(),
                "134".to_string(),
                "184".to_string(),
            ])
            .await;
        let event = vec!["18".to_string(), "S".to_string(), "184".to_string()];
        processor.process_event(event.clone()).await;
        let mut client = rxs.remove("134").unwrap();
        let received_event = block_on(client.next());
        assert_eq!(event, received_event.unwrap());
    }

    #[tokio::test]
    async fn processor_doesnt_send_status_update_message_to_matching_client_after_unfollow() {
        let (_tx, rx) = channel(5);
        let (txs, mut rxs) = seed_clients();
        let mut processor = Processor::new("127.0.0.1:0", rx).await.unwrap();
        processor.clients = txs;

        processor
            .process_event(vec![
                "15".to_string(),
                "F".to_string(),
                "354".to_string(),
                "184".to_string(),
            ])
            .await;

        let event1 = vec!["18".to_string(), "S".to_string(), "184".to_string()];
        processor.process_event(event1.clone()).await;

        processor
            .process_event(vec![
                "25".to_string(),
                "U".to_string(),
                "354".to_string(),
                "184".to_string(),
            ])
            .await;

        processor
            .process_event(vec!["28".to_string(), "S".to_string(), "184".to_string()])
            .await;
        let event2 = vec!["30".to_string(), "B".to_string()];
        processor.process_event(event2.clone()).await;

        let mut client = rxs.remove("354").unwrap();

        let received_event1 = block_on(client.next()).unwrap();

        assert_eq!(event1, received_event1);

        let received_event2 = block_on(client.next()).unwrap();
        assert_eq!(event2, received_event2);
    }
}
