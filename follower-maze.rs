extern crate sync;
extern crate green;
extern crate rustuv;

use std::io::{TcpListener, TcpStream};
use std::io::{Acceptor, Listener, IoResult};
use std::io::BufferedReader;
use std::collections::HashMap;
use sync::{Mutex, Arc, RWLock};
use std::io::EndOfFile;


// #[start]
// fn start(argc: int, argv: *const *const u8) -> int {
//     green::start(argc, argv, rustuv::event_loop, main)
// }

struct EventSourceHandler {
    events_queue: HashMap<uint, Vec<String>>,
    state: uint
}

impl EventSourceHandler {

    fn new() -> EventSourceHandler {
        EventSourceHandler{events_queue: HashMap::new(), state: 1}
    }

    fn listen_handle(&mut self, tx: Sender<Vec<String>>) -> IoResult<&'static str> {
        let listener = TcpListener::bind("127.0.0.1", 9090);
        let mut acceptor = try!(listener.listen());
        println!("Listening for events source on port 9090");
        match acceptor.accept() {
            Ok(stream) => {
                let mut reader = BufferedReader::new(stream);
                loop {
                    match reader.read_line() {
                        Ok(raw_event) => {
                            println!("raw event: {}", raw_event);
                            let event: Vec<String> = raw_event.as_slice().split('|').map(|x| x.trim().to_string()).collect();
                            println!("event read! {}", event);
                            let seq:uint = from_str(event[0].as_slice()).unwrap();
                            self.events_queue.insert(seq, event);
                            self.check_send_events(&tx);
                        },
                        Err(e) => {
                            if e.kind == EndOfFile {
                                return Err(e);
                            }
                            println!("error reading event: {}", e.kind)
                        },
                    }
                }
            }
            Err(e) => Err(e),
        }
    }
    fn check_send_events(&mut self, tx: &Sender<Vec<String>>) {
        loop {
            let mut sent = false;
            for (i, v) in self.events_queue.mut_iter() {
                let seq = v.get(0).clone();
                if seq.to_string() == self.state.to_string() {
                    println!("put event {} on the queue", v);
                    tx.send(v.clone());
                    sent = true;
                    self.state += 1;
                    //TODO delete element from map
                    // v.swap_remove(i.clone());
                }
            }
            if !sent || self.events_queue.len() == 0 {
                break;
            }
        }
        println!("loop over queued events, next round ")
    }


}

//clients handler

struct ClientsHandler {
    clients: Mutex<HashMap<String, TcpStream>>,
    followers: RWLock<HashMap<String, Vec<String>>>,
}

impl ClientsHandler {

    fn new() -> ClientsHandler {
        ClientsHandler{clients: Mutex::new(HashMap::new()), followers: RWLock::new(HashMap::new())}
    }
    fn listen(&self) -> IoResult<&'static str>{
        let listener = TcpListener::bind("127.0.0.1", 9099);
        let mut acceptor = try!(listener.listen());
        println!("Listening for clients on port 9099");
        for stream in acceptor.incoming() {
            match stream {
                Err(e) => return Err(e),
                Ok(stream) => {
                    let mut reader = BufferedReader::new(stream.clone());
                    match reader.read_line() {
                        Err(e) => {
                            println!("error reading client id: {}", e)
                        },
                        Ok(raw_id) => {
                            println!("client connected, id: {}", raw_id);
                            let id: int;
                            match from_str(raw_id.as_slice().trim()) {
                                Some(int_id) => id = int_id,
                                None => {
                                    println!("error parsing raw_id:{}", raw_id);
                                    break;
                                }
                            };
                            //get write permission on mutex
                            let mut clients_w = self.clients.lock();
                            clients_w.insert(id.to_string(), stream);
                        }
                    }
                }
            };
        }
        unreachable!();
    }
    fn handle_events(&self, rx: Receiver<Vec<String>>) {
        loop {
            let event = rx.recv();
            let output = event.connect("|") + "\n";
            match event[1].as_slice() {
                "F" => {
                    let client_id = &event[3];
                    let mut clients_r = self.clients.lock();
                    match clients_r.find(client_id) {
                        Some(client) => {
                            match client.clone().write(output.as_bytes()) {
                                Ok(_) => {
                                    println!("event: Follow, client, {}, notified ", client_id);
                                    let mut followers_w = self.followers.write();
                                    let followers = followers_w.find_or_insert(client_id.to_string(), Vec::new());
                                    followers.push(event[2].clone())
                                },
                                Err(_) => println!("{} event delivering failed", event),

                            }

                        }
                        None => println!("client not found"),
                    }
                },
                "B" => {
                    let mut clients_r = self.clients.lock();
                    for (client_id, client) in clients_r.iter() {
                        match client.clone().write(output.as_bytes()) {
                            Ok(_) => println!("{} event: broadcast, client {} notified", event, client_id),
                            Err(_) => println!("{} event delivering failed", event),
                        }
                    }
                },
                "S" => {
                    let client_id = &event[2];
                    let mut clients_r = self.clients.lock();
                    let followers_r = self.followers.read();
                    match followers_r.find(client_id) {
                        Some(client_followers) => {
                            for follower_id in client_followers.iter() {
                                let follower = clients_r.find(follower_id).unwrap();
                                match follower.clone().write(output.as_bytes()) {
                                    Ok(_) => println!("{} event: status update from {}, client {} notified ", event, client_id, follower_id),
                                    Err(_) => println!("{} event delivering failed", event),
                                }
                            }
                        },
                        None => println!("{} event delivering failed, follower {} not found", event, client_id),
                    }
                },
                "U" => {
                    let client_id = &event[3];
                    let mut followers_w = self.followers.write();
                    match followers_w.find_copy(client_id) {
                        Some(client_followers) => {
                            let unfollower_id = &event[2];
                            for follower_id in client_followers.iter() {
                                if follower_id == unfollower_id {
                                    followers_w.remove(unfollower_id);
                                }
                            }
                        },
                        None => println!("{} event delivering failed, follower {} not found", event, client_id),
                    };
                }
                _ => println!("unexpect event"),

            }
        }
    }
}

fn main() {
    let (tx, rx) = channel();
    let txc = tx.clone();
    spawn(proc() {
        let mut events_handler = EventSourceHandler::new();
        match events_handler.listen_handle(txc) {
            Err(e) => println!("Error: {}",e.desc),
            _ => unreachable!(),
        };
    });
    let clients_handler = Arc::new(ClientsHandler::new());
    let listener = clients_handler.clone();
    spawn(proc(){
        let listener = listener;
        match listener.listen() {
            Err(e) => println!("Error: {}",e.desc),
            _ => unreachable!(),
        };
    });
    let clients_events_handler = clients_handler;
    clients_events_handler.handle_events(rx);
}
