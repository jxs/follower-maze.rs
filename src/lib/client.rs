use bytes::{Buf, Bytes};
use futures::sync::mpsc::UnboundedReceiver;
use futures::try_ready;
use log::{debug, error};
use std::io::Cursor;
use tokio::io::{AsyncWrite, WriteHalf};
use tokio::net::TcpStream;
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
                        let result = self.socket.write_buf(data);
                        if let Err(err) = result {
                            error!(
                                "error sending event {} to client {}, {}",
                                event.join("|"),
                                self.id,
                                err
                            );
                            panic!();
                        }
                        try_ready!(Ok(result.unwrap()));
                    }
                    debug!("delievered event {} to client {}", event.join("|"), self.id);
                    self.state = State::Waiting;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Client;
    use futures::sync::mpsc::unbounded;
    use futures::Future;
    use std::io::BufReader;
    use tokio::io::AsyncRead;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::prelude::Stream;
    use tokio::runtime::Runtime;

    #[test]
    fn client_socket_receives_client_events() {
        env_logger::init();
        let mut rt = Runtime::new().unwrap();
        let addr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();
        let stream = TcpStream::connect(&listener.local_addr().unwrap());

        let incoming = listener
            .incoming()
            .into_future()
            .and_then(move |(socket, _rest)| {
                let (_, writer) = socket.unwrap().split();
                let (tx, rx) = unbounded();
                let client = Client::new("132".to_string(), writer, rx);
                tokio::spawn(client);
                let event = "911|P|46|68".split("|").map(|x| x.to_string()).collect();

                tx.unbounded_send(event).unwrap();
                Ok(())
            })
            .map_err(|err| {
                panic!("{:?}", err);
            });

        rt.spawn(incoming);

        let test = stream
            .and_then(move |socket| {
                let event_bytes = vec![];
                tokio::io::read_until(BufReader::new(socket), b'\n', event_bytes).and_then(
                    |(_socket, output)| {
                        let output = String::from_utf8(output).unwrap();
                        assert_eq!(output, "911|P|46|68\n");
                        Ok(())
                    },
                )
            })
            .map_err(|err| {
                panic!("{:?}", err);
            });
        rt.block_on(test).unwrap();
    }
}