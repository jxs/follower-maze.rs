extern crate env_logger;
extern crate followermaze;
extern crate futures;
extern crate tokio;

use followermaze::{client::Client, events};
use futures::sync::mpsc::unbounded;
use futures::Future;
use std::collections::HashMap;
use std::{
    sync::{Arc, RwLock}, thread, time,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::Stream;
use tokio::runtime::Runtime;

#[test]
fn events_listener_accepts_event_parses_and_sends_ordered_through_channel() {
    env_logger::init();

    let mut rt = Runtime::new().unwrap();
    let (tx, rx) = unbounded();
    let addr = "127.0.0.1:9090";

    rt.spawn(events::listen(addr, tx));
    let stream = TcpStream::connect(&addr.parse().unwrap());

    let event_send = stream
        .and_then(|socket| {
            println!("SOCKET: {:?}", socket);
            tokio::io::write_all(socket.try_clone().unwrap(), "3|F|46|74\n".as_bytes())
                .wait()
                .unwrap();

            tokio::io::write_all(socket.try_clone().unwrap(), "2|S|46\n".as_bytes())
                .wait()
                .unwrap();

            tokio::io::write_all(socket.try_clone().unwrap(), "1|B\n".as_bytes())
                .wait()
                .unwrap();

            Ok(())
        })
        .map_err(|err| {
            panic!("{:?}", err);
        });
    rt.spawn(event_send);

    let test = rx.take(3).collect().and_then(|events| {
        let mut events = events.into_iter();

        let event1 = events.next().unwrap();
        let event1_str = event1.join("|");
        assert_eq!("1|B", &event1_str);

        let event2 = events.next().unwrap();
        let event2_str = event2.join("|");
        assert_eq!("2|S|46", &event2_str);

        let event3 = events.next().unwrap();
        let event3_str = event3.join("|");
        assert_eq!("3|F|46|74", &event3_str);

        Ok(())
    });
    rt.block_on(test).unwrap();
}

fn start_get_socket() -> TcpStream {
    let addr = "127.0.0.1:0".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();
    TcpStream::connect(&listener.local_addr().unwrap())
        .wait()
        .unwrap()
}

fn seed_clients(socket: TcpStream, clients: Arc<RwLock<HashMap<String, Client>>>) {
    let mut clients_rw = clients.write().unwrap();
    let client1 = Client::new("354".to_string(), socket.try_clone().unwrap());
    clients_rw.insert("354".to_string(), client1);
    let client2 = Client::new("274".to_string(), socket.try_clone().unwrap());
    clients_rw.insert("274".to_string(), client2);
    let client3 = Client::new("184".to_string(), socket.try_clone().unwrap());
    clients_rw.insert("184".to_string(), client3);
    let client4 = Client::new("134".to_string(), socket.try_clone().unwrap());
    clients_rw.insert("134".to_string(), client4);
}

#[test]
fn events_handler_sends_broadcast_event_to_all_clients() {
    let socket = start_get_socket();
    let mut rt = Runtime::new().unwrap();

    let clients: Arc<RwLock<HashMap<String, Client>>> = Arc::new(RwLock::new(HashMap::new()));
    seed_clients(socket, clients.clone());

    let (tx, rx) = unbounded();
    tx.unbounded_send(vec!["17".to_string(), "B".to_string()])
        .unwrap();

    rt.spawn(events::handle(rx, clients.clone()));

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    let mut clients_rw = clients.write().unwrap();
    for (_id, client) in clients_rw.iter_mut() {
        let receiver = client.receiver.take().unwrap();
        let mut events = receiver.take(1).collect().wait().unwrap().into_iter();
        let event = events.next().unwrap();
        assert_eq!(vec!["17".to_string(), "B".to_string()], event);
    }
}

#[test]
fn events_handler_sends_private_message_to_matching_client() {
    let socket = start_get_socket();
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, Client>>> = Arc::new(RwLock::new(HashMap::new()));
    let (tx, rx) = unbounded();

    seed_clients(socket, clients.clone());

    rt.spawn(events::handle(rx, clients.clone()));
    tx.unbounded_send(vec![
        "15".to_string(),
        "P".to_string(),
        "46".to_string(),
        "184".to_string(),
    ]).unwrap();

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    let mut clients_rw = clients.write().unwrap();
    let client = clients_rw.get_mut("184").unwrap();
    let receiver = client.receiver.take().unwrap();
    let mut events = receiver.take(1).collect().wait().unwrap().into_iter();
    let event = events.next().unwrap();
    assert_eq!(
        vec![
            "15".to_string(),
            "P".to_string(),
            "46".to_string(),
            "184".to_string(),
        ],
        event
    );
}

#[test]
fn events_handler_sends_status_update_message_to_matching_client_after_follow() {
    let socket = start_get_socket();
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, Client>>> = Arc::new(RwLock::new(HashMap::new()));
    let (tx, rx) = unbounded();

    seed_clients(socket, clients.clone());

    rt.spawn(events::handle(rx, clients.clone()));
    tx.unbounded_send(vec![
        "15".to_string(),
        "F".to_string(),
        "134".to_string(),
        "184".to_string(),
    ]).unwrap();
    tx.unbounded_send(vec![
        "17".to_string(),
        "F".to_string(),
        "354".to_string(),
        "184".to_string(),
    ]).unwrap();
    tx.unbounded_send(vec!["18".to_string(), "S".to_string(), "184".to_string()])
        .unwrap();

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    let mut clients_rw = clients.write().unwrap();

    {
        let client184 = clients_rw.get_mut("184").unwrap();
        let receiver = client184.receiver.take().unwrap();
        let mut events = receiver.take(2).collect().wait().unwrap().into_iter();
        let event = events.next().unwrap();
        assert_eq!(
            vec![
                "15".to_string(),
                "F".to_string(),
                "134".to_string(),
                "184".to_string(),
            ],
            event
        );
        let event = events.next().unwrap();
        assert_eq!(
            vec![
                "17".to_string(),
                "F".to_string(),
                "354".to_string(),
                "184".to_string(),
            ],
            event
        );
    }

    {
        let client134 = clients_rw.get_mut("134").unwrap();
        let receiver = client134.receiver.take().unwrap();
        let mut events = receiver.take(1).collect().wait().unwrap().into_iter();
        let event = events.next().unwrap();
        assert_eq!(
            vec!["18".to_string(), "S".to_string(), "184".to_string()],
            event
        );
    }

    let client354 = clients_rw.get_mut("354").unwrap();
    let receiver = client354.receiver.take().unwrap();
    let mut events = receiver.take(1).collect().wait().unwrap().into_iter();
    let event = events.next().unwrap();
    assert_eq!(
        vec!["18".to_string(), "S".to_string(), "184".to_string()],
        event
    );
}

#[test]
fn events_handler_doesnt_send_status_update_message_to_matching_client_after_unfollow() {
    let socket = start_get_socket();
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, Client>>> = Arc::new(RwLock::new(HashMap::new()));
    let (tx, rx) = unbounded();

    seed_clients(socket, clients.clone());

    rt.spawn(events::handle(rx, clients.clone()));
    tx.unbounded_send(vec![
        "15".to_string(),
        "F".to_string(),
        "354".to_string(),
        "184".to_string(),
    ]).unwrap();
    tx.unbounded_send(vec!["18".to_string(), "S".to_string(), "184".to_string()])
        .unwrap();
    tx.unbounded_send(vec![
        "25".to_string(),
        "U".to_string(),
        "354".to_string(),
        "184".to_string(),
    ]).unwrap();
    tx.unbounded_send(vec!["28".to_string(), "S".to_string(), "184".to_string()])
        .unwrap();
    tx.unbounded_send(vec!["30".to_string(), "B".to_string()])
        .unwrap();

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    let mut clients_rw = clients.write().unwrap();

    let client354 = clients_rw.get_mut("354").unwrap();
    let receiver = client354.receiver.take().unwrap();
    let mut events = receiver.take(2).collect().wait().unwrap().into_iter();
    let event = events.next().unwrap();
    assert_eq!(
        vec!["18".to_string(), "S".to_string(), "184".to_string()],
        event
    );
    let event = events.next().unwrap();
    assert_eq!(vec!["30".to_string(), "B".to_string()], event);
}
