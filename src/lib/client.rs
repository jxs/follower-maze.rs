use futures::channel::mpsc::UnboundedReceiver;
use futures::compat::AsyncWrite01CompatExt;
use futures::prelude::AsyncWriteExt;
use futures::StreamExt;
use log::debug;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;

pub struct Client {
    id: String,
    socket: WriteHalf<TcpStream>,
    rx: UnboundedReceiver<Vec<String>>,
}

impl Client {
    pub fn new(
        id: String,
        socket: WriteHalf<TcpStream>,
        rx: UnboundedReceiver<Vec<String>>,
    ) -> Client {
        Client { id, socket, rx }
    }

    pub async fn run(mut self) {
        let mut socket = self.socket.compat();
        while let Some(event) = await!(self.rx.next()) {
            let event_str = event.join("|") + "\n";
            if let Err(err) = await!(socket.write_all(event_str.as_bytes())) {
                panic!(
                    "error sending event {} to client {}, {}",
                    event.join("|"),
                    self.id,
                    err
                )
            }
            debug!("delievered event {} to client {}", event.join("|"), self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Client;
    use futures::channel::mpsc::unbounded;
    use futures::compat::{Future01CompatExt, Stream01CompatExt};
    use futures::StreamExt;
    use tokio::codec::{FramedRead, LinesCodec};
    use tokio::io::AsyncRead;
    use tokio::net::{TcpListener, TcpStream};

    #[runtime::test(runtime_tokio::Tokio)]
    async fn client_socket_receives_client_events() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();
        let stream = TcpStream::connect(&listener.local_addr().unwrap());

        await!(async {
            let (tx, rx) = unbounded();

            runtime::spawn(async {
                let mut incoming = listener.incoming().compat();
                let socket = await!(incoming.next()).unwrap().unwrap();
                let (_, writer) = socket.split();
                let client = Client::new("132".to_string(), writer, rx);
                runtime::spawn(client.run());
            });

            let event = "911|P|46|68".split("|").map(|x| x.to_string()).collect();
            tx.unbounded_send(event).unwrap();
            let stream = await!(stream.compat()).unwrap();
            let (reader, _) = stream.split();
            let mut lines = FramedRead::new(reader, LinesCodec::new()).compat();
            let event = await!(lines.next()).unwrap().unwrap();
            assert_eq!(event, "911|P|46|68");
            assert!(true);
        });
    }
}
