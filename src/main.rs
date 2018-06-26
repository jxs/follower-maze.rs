extern crate failure;
extern crate futures;
extern crate tokio;

use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::sync::{Arc, RwLock};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::{
    future::{loop_fn, Loop}, Future, Stream,
};
use tokio::runtime::Runtime;

struct Client {
    id: String,
    socket: TcpStream,
    sender: UnboundedSender<Vec<String>>,
    receiver: Option<UnboundedReceiver<Vec<String>>>
}

impl Client {
    fn new(id: String, socket: TcpStream) -> Client {
        let (sender, receiver) = unbounded();
        Client {
            id: id,
            socket: socket,
            sender: sender,
            receiver: Some(receiver)
        }
    }

    fn send(&mut self, event: Vec<String>) {
        self.sender.unbounded_send(event.clone()).expect(&format!(
            "error sending event: {} to client: {}, receiver not available aborting",
            event[0], self.id
        ));
    }

    fn run(&mut self) -> impl Future<Item = (), Error = ()> {
        let id = self.id.clone();
        let socket = self.socket.try_clone().unwrap();
        let receiver = self.receiver.take();
        let receiver = receiver.unwrap();

        receiver.for_each(move |event| {
            let event_str = event.join("|");
            println!("client: {} - received event: {}", id, event_str);
            let output = event_str.clone() + "\n";
            let id = id.clone();

            if event[1].as_str() == "U" {
                return Ok(());
            }

            tokio::io::write_all(socket.try_clone().unwrap(), output.as_bytes().to_vec())
                .wait()
                .and_then(|_res| {
                    println!("client: {} - event {} delievered", &id, &event_str);
                    Ok(())
                })
                .expect(&format!(
                    "client:{} error delievering event: {}, socket closed, aborting",
                    event_str.clone(),
                    id.clone()
                ));
            Ok(())
        })
    }
}

fn events_listener(tx: UnboundedSender<Vec<String>>) -> impl Future<Item = (), Error = ()> {
    let addr = "127.0.0.1:9090".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    println!("Listening for events source on port 9090");
    listener
        .incoming()
        .for_each(move |socket| {
            println!("events listener: connected");
            let event_stream_loop = loop_fn(
                (1, HashMap::new(), tx.clone(), BufReader::new(socket)),
                |(mut state, mut events_queue, tx, reader)| {
                    tokio::io::read_until(reader, b'\n', Vec::new())
                        .and_then(move |(reader, raw_event)| {
                            let event_str = String::from_utf8(raw_event).unwrap();
                            if event_str.is_empty() {
                                return Ok(Loop::Break(()));
                            }
                            // println!("events listener: read event: {}", event_str);
                            let event: Vec<String> =
                                event_str.trim().split('|').map(|x| x.to_string()).collect();
                            let seq: usize = event[0].parse().unwrap();
                            // println!("events listener: inserted event: {} -- {:?}", seq, event);
                            events_queue.insert(seq, event);
                            loop {
                                match events_queue.get(&state) {
                                    Some(event) => {
                                        tx.unbounded_send(event.clone()).expect(&format!(
                                            "events listener: error sending event: {} aborting",
                                            event_str
                                        ));
                                        state += 1;
                                        // println!(
                                        //     "events listener: sent event : {}, state: {}",
                                        //     event.join("|"),
                                        //     state
                                        // );
                                    }
                                    None => {
                                        break;
                                    }
                                };
                            }

                            Ok(Loop::Continue((state, events_queue, tx, reader)))
                        })
                        .map_err(|err| {
                            println!("server error {:?}", err);
                        })
                },
            );
            tokio::spawn(event_stream_loop);
            Ok(())
        })
        .map_err(|err| {
            println!("server error {:?}", err);
        })
}

fn clients_listener(
    clients: Arc<RwLock<HashMap<String, Client>>>,
) -> impl Future<Item = (), Error = ()> {
    let addr = "127.0.0.1:9099".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    println!("Listening for clients on port 9099");
    listener
        .incoming()
        .for_each(move |socket| {
            //move clients to this closure
            let clients = Arc::clone(&clients);
            let events = Vec::new();
            let reader = BufReader::new(socket.try_clone().unwrap());
            let futu = tokio::io::read_until(reader, b'\n', events)
                .map_err(|err| {
                    println!("server error {:?}", err);
                })
                .and_then(move |(_bfsocket, bclient)| {
                    let client_id = String::from_utf8(bclient).unwrap().trim().to_string();
                    debug!(target: "clients listener", "client connected: {:?}", client_id);
                    let mut client = Client::new(client_id.clone(), socket);
                    tokio::spawn(client.run());
                    let mut clients_rw = clients.write().unwrap();
                    clients_rw.insert(client_id, client);
                    Ok(())
                });

            tokio::spawn(futu);
            Ok(())
        })
        .map_err(|err| {
            println!("server error {:?}", err);
        })
}

fn events_handler(
    tx: UnboundedReceiver<Vec<String>>,
    clients: Arc<RwLock<HashMap<String, Client>>>,
) -> impl Future<Item = (), Error = ()> {
    let followers: Arc<RwLock<HashMap<String, HashSet<String>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    tx.for_each(move |event| {
        let mut clients = clients.write().unwrap();
        let event_str = event.join("|");
        match event[1].as_str() {
            "P" => {
                let client_id = event[3].clone();
                match clients.get_mut(&client_id) {
                    Some(client) => {
                        client.send(event.clone());
                    }
                    _ => println!(
                        "events handler: skipping event {}, client {} not found",
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
                        let followers = followers.entry(client_id.clone()).or_insert(HashSet::new());
                        followers.insert(event[2].clone());

                        println!(
                        "events handler: skipping event {}, client {} not found, but adding to its follower list",
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
                    None => println!(
                        "events handler: skipping unfollow, client: {} isn't followed by {}",
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
                        println!(
                            "events handler: skipping sending event {}, client:{} not found",
                            event_str, client_id
                        );
                        return Ok(());
                    }
                };

                println!("client: {} followers: {:?}", client_id, followers);
                for follower_id in followers.iter() {
                    match clients.get_mut(follower_id) {
                        Some(follower) => follower.send(event.clone()),
                        None => {
                            println!(
                                "events handler: skipping sending event {}, follower:{} not found",
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

fn main() {
    let (tx, rx) = unbounded();
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, Client>>> = Arc::new(RwLock::new(HashMap::new()));

    rt.spawn(events_listener(tx));
    rt.spawn(clients_listener(clients.clone()));
    rt.spawn(events_handler(rx, clients));
    rt.shutdown_on_idle().wait().unwrap();
}
