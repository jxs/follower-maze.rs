extern crate env_logger;
extern crate followermaze;
extern crate futures;
extern crate tokio;

use followermaze::{client, client::Client};
use futures::Future;
use std::collections::HashMap;
use std::{
    sync::{Arc, RwLock}, thread, time,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::Stream;
use tokio::runtime::Runtime;

#[test]
fn socket_receives_client_events() {
    env_logger::init();
    let mut rt = Runtime::new().unwrap();
    let addr = "127.0.0.1:0".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();
    let stream = TcpStream::connect(&listener.local_addr().unwrap());

    let incoming = listener
        .incoming()
        .take(1)
        .collect()
        .and_then(|sockets| {
            let socket = sockets.into_iter().next().unwrap();
            let mut client = Client::new("354".to_string(), socket.try_clone().unwrap());
            let sent_event = "911|P|46|68"
                .to_string()
                .split("|")
                .map(|x| x.to_string())
                .collect();
            client.send(sent_event);

            tokio::spawn(client.run());
            Ok(())
        })
        .map_err(|err| {
            panic!("{:?}", err);
        });

    rt.spawn(incoming);

    let test = stream
        .and_then(|socket| {
            let event_bytes = vec![];
            tokio::io::read_to_end(socket, event_bytes.clone()).and_then(|(_socket, output)| {
                let output = String::from_utf8(output).unwrap();
                assert_eq!(output, "911|P|46|68\n");
                Ok(())
            })
        })
        .map_err(|err| {
            panic!("{:?}", err);
        });
    rt.block_on(test).unwrap();
}

#[test]
fn clients_listener_adds_clients_to_hashmap() {
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, Client>>> = Arc::new(RwLock::new(HashMap::new()));
    rt.spawn(client::listen("127.0.0.1:9099", clients.clone()));

    let test = TcpStream::connect(&"127.0.0.1:9099".parse().unwrap())
        .and_then(move |socket| {
            tokio::io::write_all(socket.try_clone().unwrap(), "355".as_bytes().to_vec())
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
