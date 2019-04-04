use bytes::{Buf, Bytes};
use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::try_ready;
use log::{debug, error, info};
use std::collections::HashMap;
use std::io::BufReader;
use std::io::Cursor;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncRead, AsyncWrite, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::{Async, Future, Poll, Stream};

enum State {
    // waiting for events
    Waiting,
    // writing event on socket to client
    Writing(Vec<String>, Cursor<Bytes>),
}

pub struct Client {
    id: String,
    socket: WriteHalf<TcpStream>,
    rx: UnboundedReceiver<Vec<String>>,
    state: State,
}

impl Client {
    pub fn new(
        id: String,
        socket: WriteHalf<TcpStream>,
        rx: UnboundedReceiver<Vec<String>>,
    ) -> Client {
        Client {
            id,
            socket,
            rx,
            state: State::Waiting,
        }
    }
}

impl Future for Client {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            match self.state {
                State::Waiting => match try_ready!(self.rx.poll()) {
                    Some(event) => {
                        let event_str = event.join("|") + "\n";
                        let data = Cursor::new(Bytes::from(event_str));
                        self.state = State::Writing(event, data);
                    }
                    None => return Ok(Async::Ready(())),
                },
                State::Writing(ref event, ref mut data) => {
                    while data.has_remaining() {
                        if let Err(err) = self.socket.write_buf(data) {
                            error!(
                                "error sending event {} to client {}, {}",
                                event.join("|"),
                                self.id,
                                err
                            );
                            panic!();
                        }
                    }
                    debug!("delievered event {} to client {}", event.join("|"), self.id);
                    self.state = State::Waiting;
                }
            }
        }
    }
}

pub fn listen<S: ::std::hash::BuildHasher>(
    addr: &str,
    clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>, S>>>,
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
                let (tx, rx) = unbounded();
                let client = Client::new(client_id.clone(), writer, rx);
                tokio::spawn(client);
                let mut clients_rw = clients.write().unwrap();
                clients_rw.insert(client_id, tx);
                Ok(())
            })
        })
        .map_err(|err| {
            error!("clients listener, error {:?}", err);
        })
}
