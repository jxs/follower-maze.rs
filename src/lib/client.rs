use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use std::collections::HashMap;
use std::io::BufReader;
use std::sync::{Arc, RwLock};
use tokio;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::{Future, Stream};

pub struct Client {
    id: String,
    socket: TcpStream,
    sender: UnboundedSender<Vec<String>>,
    receiver: Option<UnboundedReceiver<Vec<String>>>,
}

impl Client {
    pub fn new(id: String, socket: TcpStream) -> Client {
        let (sender, receiver) = unbounded();
        Client {
            id: id,
            socket: socket,
            sender: sender,
            receiver: Some(receiver),
        }
    }

    pub fn send(&self, event: Vec<String>) {
        self.sender
            .unbounded_send(event.clone())
            .unwrap_or_else(|err| {
                error!(
                    target: &format!("client: {}", self.id),
                    "error deliervering event {}: {}",
                    event.join("|"),
                    err
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
            debug!(
                target: &format!("client: {}", id),
                "received event: {}",
                event_str
            );
            let output = event_str.clone() + "\n";
            let id = id.clone();

            if event[1].as_str() == "U" {
                return Ok(());
            }

            tokio::io::write_all(socket.try_clone().unwrap(), output.as_bytes().to_vec())
                .wait()
                .and_then(|_res| {
                    info!(
                        target: &format!("client: {}", &id),
                        "delievered event {}",
                        &event_str
                    );
                    Ok(())
                })
                .unwrap_or_else(|err| {
                    error!(
                        target: &format!("client: {}", id.clone()),
                        "error delievering event: {} : {}",
                        event_str.clone(),
                        err
                    );
                    panic!()
                });
            Ok(())
        })
    }
}

pub fn listen(
    addr: &str,
    clients: Arc<RwLock<HashMap<String, Client>>>,
) -> impl Future<Item = (), Error = ()> {
    let addrf = addr.parse().unwrap();
    let listener = TcpListener::bind(&addrf).unwrap();

    info!(target: "clients listener", "Listening for clients on {}", addr);
    listener
        .incoming()
        .for_each(move |socket| {
            //move clients to this closure
            let clients = Arc::clone(&clients);
            let events = Vec::new();
            let reader = BufReader::new(socket.try_clone().unwrap());
            let futu = tokio::io::read_until(reader, b'\n', events)
                .map_err(|err| {
                    error!(target:"clients listener", "error {:?}", err);
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
            error!(target:"clients listener", "error {:?}", err);
        })
}
