use followermaze::events::{self, Processor};
use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::Future;
use std::collections::HashMap;
use std::{
    sync::{Arc, RwLock},
    thread, time,
};
use tokio::net::TcpStream;
use tokio::prelude::Stream;
use tokio::runtime::Runtime;

#[test]
fn events_listener_accepts_event_parses_and_sends_ordered_through_channel() {
    env_logger::init();

    let mut rt = Runtime::new().unwrap();
    let (tx, rx) = unbounded();
    let addr = "127.0.0.1:9090";

    rt.spawn(events::Streamer::new(addr, tx).unwrap());
    let stream = TcpStream::connect(&addr.parse().unwrap());

    let event_send = stream
        .and_then(|socket| {
            tokio::io::write_all(socket, "3|F|46|74\n".as_bytes())
                .and_then(|(socket, _buf)| tokio::io::write_all(socket, "2|S|46\n".as_bytes()))
                .and_then(|(socket, _buf)| tokio::io::write_all(socket, "1|B\n".as_bytes()))
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

fn seed_clients(
    clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>>>>,
) -> HashMap<String, UnboundedReceiver<Vec<String>>> {
    let mut clients_rw = clients.write().unwrap();
    let mut rxs = HashMap::new();

    let (tx, rx) = unbounded();
    clients_rw.insert("354".to_string(), tx);
    rxs.insert("354".to_string(), rx);

    let (tx, rx) = unbounded();
    clients_rw.insert("274".to_string(), tx);
    rxs.insert("274".to_string(), rx);

    let (tx, rx) = unbounded();
    clients_rw.insert("184".to_string(), tx);
    rxs.insert("184".to_string(), rx);

    let (tx, rx) = unbounded();
    clients_rw.insert("134".to_string(), tx);
    rxs.insert("134".to_string(), rx);
    rxs
}

#[test]
fn events_handler_sends_broadcast_event_to_all_clients() {
    let mut rt = Runtime::new().unwrap();

    let clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let rxs = seed_clients(clients.clone());

    let (tx, rx) = unbounded();
    tx.unbounded_send(vec!["17".to_string(), "B".to_string()])
        .unwrap();

    rt.spawn(Processor::new(rx, clients.clone()));

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    for (_id, rx) in rxs {
        // let receiver = client.rx.take().unwrap();
        let mut events = rx.take(1).collect().wait().unwrap().into_iter();
        let event = events.next().unwrap();
        assert_eq!(vec!["17".to_string(), "B".to_string()], event);
    }
}

#[test]
fn events_handler_sends_private_message_to_matching_client() {
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let (tx, rx) = unbounded();

    let mut rxs = seed_clients(clients.clone());

    rt.spawn(Processor::new(rx, clients.clone()));
    tx.unbounded_send(vec![
        "15".to_string(),
        "P".to_string(),
        "46".to_string(),
        "184".to_string(),
    ])
    .unwrap();

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    let rx = rxs.remove("184").unwrap();
    let mut events = rx.take(1).collect().wait().unwrap().into_iter();
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
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let (tx, rx) = unbounded();

    let mut rxs = seed_clients(clients.clone());

    rt.spawn(Processor::new(rx, clients.clone()));
    tx.unbounded_send(vec![
        "15".to_string(),
        "F".to_string(),
        "134".to_string(),
        "184".to_string(),
    ])
    .unwrap();
    tx.unbounded_send(vec![
        "17".to_string(),
        "F".to_string(),
        "354".to_string(),
        "184".to_string(),
    ])
    .unwrap();
    tx.unbounded_send(vec!["18".to_string(), "S".to_string(), "184".to_string()])
        .unwrap();

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    {
        let rx = rxs.remove("184").unwrap();
        let mut events = rx.take(2).collect().wait().unwrap().into_iter();
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
        let rx = rxs.remove("134").unwrap();
        let mut events = rx.take(1).collect().wait().unwrap().into_iter();
        let event = events.next().unwrap();
        assert_eq!(
            vec!["18".to_string(), "S".to_string(), "184".to_string()],
            event
        );
    }

    let rx = rxs.remove("354").unwrap();
    let mut events = rx.take(1).collect().wait().unwrap().into_iter();
    let event = events.next().unwrap();
    assert_eq!(
        vec!["18".to_string(), "S".to_string(), "184".to_string()],
        event
    );
}

#[test]
fn events_handler_doesnt_send_status_update_message_to_matching_client_after_unfollow() {
    let mut rt = Runtime::new().unwrap();
    let clients: Arc<RwLock<HashMap<String, UnboundedSender<Vec<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let (tx, rx) = unbounded();

    let mut rxs = seed_clients(clients.clone());

    rt.spawn(Processor::new(rx, clients.clone()));
    tx.unbounded_send(vec![
        "15".to_string(),
        "F".to_string(),
        "354".to_string(),
        "184".to_string(),
    ])
    .unwrap();
    tx.unbounded_send(vec!["18".to_string(), "S".to_string(), "184".to_string()])
        .unwrap();
    tx.unbounded_send(vec![
        "25".to_string(),
        "U".to_string(),
        "354".to_string(),
        "184".to_string(),
    ])
    .unwrap();
    tx.unbounded_send(vec!["28".to_string(), "S".to_string(), "184".to_string()])
        .unwrap();
    tx.unbounded_send(vec!["30".to_string(), "B".to_string()])
        .unwrap();

    // wait to allow for events::handle to aquire write lock
    thread::sleep(time::Duration::from_millis(10));

    let rx = rxs.remove("354").unwrap();
    let mut events = rx.take(2).collect().wait().unwrap().into_iter();
    let event = events.next().unwrap();
    assert_eq!(
        vec!["18".to_string(), "S".to_string(), "184".to_string()],
        event
    );
    let event = events.next().unwrap();
    assert_eq!(vec!["30".to_string(), "B".to_string()], event);
}
