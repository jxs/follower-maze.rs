use log::debug;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct Client {
    id: String,
    socket: TcpStream,
    rx: UnboundedReceiver<Vec<String>>,
}

impl Client {
    pub fn new(id: String, socket: TcpStream, rx: UnboundedReceiver<Vec<String>>) -> Client {
        Client { id, socket, rx }
    }

    pub async fn run(mut self) {
        while let Some(event) = self.rx.recv().await {
            let event_str = event.join("|") + "\n";
            if let Err(err) = self.socket.write_all(event_str.as_bytes()).await {
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
    use futures::StreamExt;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::mpsc::unbounded_channel;
    use tokio_util::codec::{FramedRead, LinesCodec};

    #[tokio::test]
    async fn client_socket_receives_client_events() {
        let addr = "127.0.0.1:0";
        let listener = TcpListener::bind(&addr).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = TcpStream::connect(&addr);

        let (tx, rx) = unbounded_channel();

        tokio::spawn(async move {
            let (socket, _addr) = listener.accept().await.unwrap();
            let client = Client::new("132".to_string(), socket, rx);
            tokio::spawn(client.run());
        });

        let event = "911|P|46|68".split("|").map(|x| x.to_string()).collect();
        tx.send(event).unwrap();
        let mut stream = stream.await.unwrap();
        let (reader, _) = stream.split();
        let mut lines = FramedRead::new(reader, LinesCodec::new());
        let event = lines.next().await.unwrap().unwrap();
        assert_eq!(event, "911|P|46|68");
        assert!(true);
    }
}
