use bytes::Bytes;
use futures::sink::Sink;
use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use log::{debug, error, info};
use std::collections::HashMap;
use std::io::BufReader;
use std::sync::{Arc, RwLock};
use tokio::codec::{BytesCodec, FramedWrite};
use tokio::io::{AsyncRead, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::{Future, Stream};

pub struct Client {
    pub id: String,
    pub socket: Option<WriteHalf<TcpStream>>,
    pub sender: UnboundedSender<Vec<String>>,
    pub receiver: Option<UnboundedReceiver<Vec<String>>>,
}

impl Client {
    pub fn new(id: String, socket: WriteHalf<TcpStream>) -> Client {
        let (sender, receiver) = unbounded();
        Client {
            id,
            socket: Some(socket),
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

        let socket = self.socket.take().expect("run can only be called once");
        let receiver = self.receiver.take().expect("run can only be called once");

        let messages_stream = receiver
            .map({
                move |event| {
                    let event_str = event.join("|");
                    let output = event_str.clone() + "\n";
                    Bytes::from(output)
                }
            })
            .inspect({
                let id = id.clone();
                move |event| {
                    info!(
                        "client {} received event: {}",
                        id,
                        String::from_utf8(event.to_vec()).unwrap()
                    );
                }
            });

        let framed = FramedWrite::new(socket, BytesCodec::new()).sink_map_err(|_err| ());
        framed.send_all(messages_stream).map(|_out| ()).map_err({
            let id = id.clone();
            //TODO ATTACH THE EVENT THAT FAILED
            move |err| {
                error!(
                    "client {} error delivering event {} {:?}",
                    id.clone(),
                    "FAILED EVENT",
                    err
                );
                panic!();
            }
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
            let (reader, writer) = socket.split();
            let reader = BufReader::new(reader);
            tokio::io::read_until(reader, b'\n', events).and_then(move |(_bfsocket, bclient)| {
                let client_id = String::from_utf8(bclient).unwrap().trim().to_string();
                debug!("clients listener client connected: {:?}", client_id);
                let mut client = Client::new(client_id.clone(), writer);
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
