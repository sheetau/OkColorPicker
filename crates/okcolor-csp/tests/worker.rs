use okcolor_csp::{
    Pairing,
    frame::{Decoder, Frame, Kind},
    qr::ScanError,
    worker::{Event, State, Worker},
};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(4);
fn pairing(port: u16) -> Pairing {
    let raw = format!("127.0.0.1\t{port}\tsecretpw\tG#1:2026");
    let key = [0x74, 0xb2, 0x92, 0x5b, 0x4a, 0x21, 0xda];
    let encoded: String = raw
        .bytes()
        .enumerate()
        .map(|(i, b)| format!("{:02x}", b ^ key[i % 7]))
        .collect();
    Pairing::parse(&format!(
        "https://companion.clip-studio.com/rc/en-us?s={encoded}"
    ))
    .unwrap()
}
fn receive(socket: &mut BufReader<TcpStream>) -> Frame {
    let mut bytes = vec![];
    assert!(socket.read_until(0, &mut bytes).unwrap() > 0);
    Decoder::default().feed(&bytes).unwrap().pop().unwrap()
}
fn reply(socket: &mut BufReader<TcpStream>, frame: &Frame) {
    socket
        .get_mut()
        .write_all(
            &Frame {
                kind: Kind::Success,
                command: frame.command.clone(),
                serial: frame.serial,
                detail: if frame.command == "Authenticate" {
                    Some(json!([true, false]))
                } else {
                    None
                },
                data: vec![],
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
}
fn accept(listener: &TcpListener) -> BufReader<TcpStream> {
    let deadline = Instant::now() + WAIT;
    listener.set_nonblocking(true).unwrap();
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("Mock accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    stream.set_write_timeout(Some(WAIT)).unwrap();
    BufReader::new(stream)
}
fn state(events: &mpsc::Receiver<(u64, Event)>, generation: u64, expected: State) {
    let deadline = Instant::now() + WAIT;
    loop {
        let (actual, event) = events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        assert_eq!(actual, generation);
        if let Event::State(value) = event {
            assert_eq!(value, expected);
            return;
        }
    }
}

#[test]
fn cancelled_scan_cannot_publish_a_late_error_and_worker_can_be_reused() {
    let (started_tx, started) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    let worker = Worker::start(
        move |cancelled| {
            started_tx.send(()).unwrap();
            release.recv_timeout(WAIT).unwrap();
            assert!(cancelled());
            Err(ScanError::NoPairing)
        },
        move |generation, event| {
            tx.send((generation, event)).unwrap();
        },
    )
    .unwrap();
    let control = worker.control();
    control.scan();
    started.recv_timeout(WAIT).unwrap();
    state(&rx, 1, State::Scanning);
    control.disconnect();
    release_tx.send(()).unwrap();
    state(&rx, 2, State::Idle);
    control.disconnect();
    state(&rx, 3, State::Idle);
    drop(worker);
    assert!(rx.try_recv().is_err());
}

#[test]
fn cancellation_during_authentication_discards_late_success_and_closes_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let pairing = pairing(listener.local_addr().unwrap().port());
    let (ready_tx, ready) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept(&listener);
        let auth = receive(&mut socket);
        ready_tx.send(()).unwrap();
        release.recv_timeout(WAIT).unwrap();
        reply(&mut socket, &auth);
        assert_eq!(socket.read(&mut [0]).unwrap(), 0);
    });
    let (tx, rx) = mpsc::channel();
    let worker = Worker::start(
        |_| Err(ScanError::NoPairing),
        move |g, e| {
            tx.send((g, e)).unwrap();
        },
    )
    .unwrap();
    let control = worker.control();
    control.pair(pairing);
    ready.recv_timeout(WAIT).unwrap();
    state(&rx, 1, State::Connecting);
    control.disconnect();
    release_tx.send(()).unwrap();
    state(&rx, 2, State::Idle);
    drop(worker);
    server.join().unwrap();
    assert!(rx.try_recv().is_err());
}

#[test]
fn worker_maintains_heartbeat_reconnects_and_releases_idle_socket_on_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let pairing = pairing(listener.local_addr().unwrap().port());
    let (done_tx, done) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept(&listener);
        let auth = receive(&mut socket);
        assert_eq!(auth.command, "Authenticate");
        let token = auth.detail.as_ref().unwrap()[2].clone();
        reply(&mut socket, &auth);
        let heartbeat = receive(&mut socket);
        assert_eq!(heartbeat.command, "TellHeartbeat");
        reply(&mut socket, &heartbeat);
        drop(socket);
        let mut socket = accept(&listener);
        let auth = receive(&mut socket);
        assert_eq!(auth.command, "Authenticate");
        assert_eq!(auth.detail.as_ref().unwrap()[2], token);
        reply(&mut socket, &auth);
        done_tx.send(()).unwrap();
        assert_eq!(socket.read(&mut [0]).unwrap(), 0);
    });
    let (tx, rx) = mpsc::channel();
    let worker = Worker::start(
        |_| Err(ScanError::NoPairing),
        move |g, e| {
            tx.send((g, e)).unwrap();
        },
    )
    .unwrap();
    worker.control().pair(pairing);
    state(&rx, 1, State::Connecting);
    state(&rx, 1, State::Connected);
    state(&rx, 2, State::Reconnecting);
    state(&rx, 2, State::Connected);
    assert_eq!(worker.control().generation(), 2);
    done.recv_timeout(WAIT).unwrap();
    let start = Instant::now();
    drop(worker);
    assert!(start.elapsed() < Duration::from_secs(1));
    server.join().unwrap();
}
