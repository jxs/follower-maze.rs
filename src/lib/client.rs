use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use std::collections::HashMap;
use std::io::BufReader;
use std::sync::{Arc, RwLock};
use tokio;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::{Future, Stream};
use log::{log, debug, error, info};

pub struct Client {
    pub id: String,
    pub socket: TcpStream,
    pub sender: UnboundedSender<Vec<String>>,
    pub receiver: Option<UnboundedReceiver<Vec<String>>>,
}

impl Client {
    pub fn new(id: String, socket: TcpStream) -> Client {
        let (sender, receiver) = unbounded();
        Client {
            id,
            socket,
            sender,
            receiver: Some(receiver),
        }
    }

    pub fn send(&self, event: Vec<String>) {
        let event_str = event.join("|");
        self.sender.unbounded_send(event).unwrap_or_else(|err| {
            error!(
                "error deliervering event {} to client {},  {}",
                event_str, self.id, err
            );
            panic!()
        });
    }

    pub fn run(&mut self) -> impl Future<Item = (), Error = ()> {
        let id = self.id.clone();
        let socket = self.socket.try_clone().unwrap();
        let receiver = self.receiver.take();
        let receiver = receiver.expect("run can only be called once");

        receiver.for_each(move |event| {
            let event_str = event.join("|");
            debug!("client {} received event: {}", id, event_str);
            let output = event_str.clone() + "\n";
            let id = id.clone();

            tokio::io::write_all(socket.try_clone().unwrap(), output.as_bytes().to_vec())
                .and_then({
                    let id = id.clone();
                    let event_str = event_str.clone();
                    move |_res| {
                        info!(
                            "client {} delievered event {}",
                            id.clone(),
                            event_str.clone()
                        );
                        Ok(())
                    }
                })
                .map_err({
                    let id = id.clone();
                    let event_str = event_str.clone();
                    move |err| {
                        error!(
                            "client {} error delivering event {} {:?}",
                            id.clone(),
                            event_str.clone(),
                            err
                        );
                        panic!();
                    }
                })
        })
    }
}

pub fn listen<S: ::std::hash::BuildHasher>(
    addr: &str,
    clients: Arc<RwLock<HashMap<String, Client, S>>>,
) -> impl Future<Item = (), Error = ()> {
    let addrf = addr.parse().unwrap();
    let listener = TcpListener::bind(&addrf).unwrap();

    info!("clients listener Listening for clients on {}", addr);
    listener
        .incoming()
        .for_each(move |socket| {
            //move clients to this closure
            let clients = Arc::clone(&clients);
            let events = Vec::new();
            let reader = BufReader::new(socket.try_clone().unwrap());
            tokio::io::read_until(reader, b'\n', events).and_then(move |(_bfsocket, bclient)| {
                let client_id = String::from_utf8(bclient).unwrap().trim().to_string();
                debug!("clients listener client connected: {:?}", client_id);
                let mut client = Client::new(client_id.clone(), socket);
                tokio::spawn(client.run());
                let mut clients_rw = clients.write().unwrap();
                clients_rw.insert(client_id, client);
                Ok(())
            })
        })
        .map_err(|err| {
            error!("clients listener, error {:?}", err);
        })
}
