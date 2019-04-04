use followermaze::{client, client::Client};
use futures::sync::mpsc::{unbounded, UnboundedSender};
use futures::Future;
use std::collections::HashMap;
use std::{
    io::BufReader,
    sync::{Arc, RwLock},
    thread, time,
};
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
            println!("client connected! : {:?}", socket);
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
            println!("connected to socket!");
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

#[test]
fn clients_listener_adds_clients_to_hashmap() {
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    rt.spawn(client::listen("127.0.0.1:9099", clients.clone()));

    let test = TcpStream::connect(&"127.0.0.1:9099".parse().unwrap())
        .and_then(move |socket| {
            tokio::io::write_all(socket, "355".as_bytes().to_vec())
                .wait()
                .unwrap();
            Ok(())
        })
        .map_err(|err| {
            panic!("{:?}", err);
        });

    rt.block_on(test).unwrap();
    thread::sleep(time::Duration::from_millis(10));
    let clients = clients.read().unwrap();
    let client = clients.get("355");
    assert_eq!(1, clients.len());
    assert_eq!(true, client.is_some());
}
