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

#[test]
fn color_pushes_are_delivered_before_the_outstanding_heartbeat_reply() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let credentials = pairing(listener.local_addr().unwrap().port());
    let (observed_tx, observed) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept(&listener);
        let auth = receive(&mut socket);
        reply(&mut socket, &auth);
        let heartbeat = receive(&mut socket);
        assert_eq!(heartbeat.command, "TellHeartbeat");
        for index in 0..4u32 {
            let push = Frame {
                kind: Kind::Command,
                command: "SyncColorCircleUIState".into(),
                serial: 100 + index,
                detail: Some(json!({"CurrentColorIndex":0,"ColorSelectionModel":"HLS",
                    "HLSColorMainH":index * 400000000,"HLSColorMainL":2147483648u32,"HLSColorMainS":4294967295u32,
                    "HLSColorSubH":0,"HLSColorSubL":4294967295u32,"HLSColorSubS":0,
                    "IsCurrentColorTransparent":false,"CanExecuteSetterCommand":true,"IsManipulating":true})),
                data: vec![],
            };
            socket.get_mut().write_all(&push.encode().unwrap()).unwrap();
            let ack = receive(&mut socket);
            assert_eq!((ack.kind, ack.serial), (Kind::Success, push.serial));
            // The test releases the next push only after its UI-facing event. The
            // heartbeat ACK remains withheld throughout, so batching would deadlock.
            assert_eq!(observed.recv_timeout(WAIT).unwrap(), index);
        }
        reply(&mut socket, &heartbeat);
    });
    let (tx, rx) = mpsc::channel();
    let worker = Worker::start(
        |_| Err(ScanError::NoPairing),
        move |g, e| {
            tx.send((g, e)).unwrap();
        },
    )
    .unwrap();
    worker.control().pair(credentials);
    state(&rx, 1, State::Connecting);
    state(&rx, 1, State::Connected);
    for index in 0..4 {
        let (generation, event) = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(generation, 1);
        let Event::Color {
            state,
            after_write: 0,
        } = event
        else {
            panic!("expected immediate color push")
        };
        assert!(state.manipulating);
        assert_eq!(state.colors[1], Some([1.0; 3]));
        observed_tx.send(index).unwrap();
    }
    drop(worker);
    server.join().unwrap();
}
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
        loop {
            let mut bytes = vec![];
            match socket.read_until(0, &mut bytes) {
                Ok(0) => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    break;
                }
                Ok(_) => {}
                Err(e) => panic!("socket remained open: {e}"),
            }
            let frame = Decoder::default().feed(&bytes).unwrap().pop().unwrap();
            assert!(matches!(
                frame.command.as_str(),
                "TellHeartbeat" | "SyncColorCircleUIState" | "SyncSubViewUIState"
            ));
            reply(&mut socket, &frame);
        }
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

#[test]
fn color_sync_uses_active_gate_coalesces_writes_and_orders_pushes_before_ack() {
    use okcolor_csp::color::ColorCommand;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let pairing = pairing(listener.local_addr().unwrap().port());
    let (reading_tx, reading) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept(&listener);
        let auth = receive(&mut socket);
        reply(&mut socket, &auth);
        let heartbeat = receive(&mut socket);
        assert_eq!(heartbeat.command, "TellHeartbeat");
        assert_eq!(
            heartbeat.detail,
            Some(json!({"IdleTimerResetRequested":true}))
        );
        reply(&mut socket, &heartbeat);
        let request = receive(&mut socket);
        assert_eq!(request.command, "SyncColorCircleUIState");
        reading_tx.send(()).unwrap();
        release.recv_timeout(WAIT).unwrap();
        let mut state = json!({"CurrentColorIndex":0,"ColorSelectionModel":"HSV",
            "HSVColorMainH":0,"HSVColorMainS":4294967295u32,"HSVColorMainV":4294967295u32,
            "IsCurrentColorTransparent":false,"CanExecuteSetterCommand":true,"IsManipulating":false});
        let mut response = Frame {
            kind: Kind::Success,
            command: request.command,
            serial: request.serial,
            detail: Some(state.clone()),
            data: vec![],
        };
        socket
            .get_mut()
            .write_all(&response.encode().unwrap())
            .unwrap();
        let set = receive(&mut socket);
        assert_eq!(set.command, "SetCurrentColor");
        assert_eq!(
            set.detail,
            Some(json!({"ColorSpaceKind":"HSV","ColorIndex":0,
            "HSVColorH":2863311530u32,"HSVColorS":4294967295u32,"HSVColorV":4294967295u32,"IsColorTransparent":false}))
        );
        // A previous state push arrives during the setter exchange, before its ACK.
        response.kind = Kind::Command;
        response.serial = 99;
        socket
            .get_mut()
            .write_all(&response.encode().unwrap())
            .unwrap();
        let ack = receive(&mut socket);
        assert_eq!((ack.kind, ack.serial), (Kind::Success, 99));
        reply(&mut socket, &set);
        let poll = receive(&mut socket);
        assert_eq!(poll.command, "SyncColorCircleUIState");
        state["HSVColorMainH"] = json!(2863311530u32);
        response.kind = Kind::Success;
        response.serial = poll.serial;
        response.detail = Some(state);
        socket
            .get_mut()
            .write_all(&response.encode().unwrap())
            .unwrap();
        let sub = receive(&mut socket);
        assert_eq!(sub.command, "SetCurrentColor");
        assert_eq!(sub.detail.as_ref().unwrap()["ColorIndex"], 1);
        assert!(
            sub.detail
                .as_ref()
                .unwrap()
                .get("CurrentToolAlphaPercent")
                .is_none()
        );
        reply(&mut socket, &sub);
        // Shutdown can cancel the next confirmation read.
        let mut bytes = [0; 4096];
        loop {
            match socket.read(&mut bytes) {
                Ok(0) => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    break;
                }
                Ok(_) => {}
                Err(e) => panic!("socket remained open: {e}"),
            }
        }
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
    state(&rx, 1, State::Connecting);
    state(&rx, 1, State::Connected);
    reading.recv_timeout(WAIT).unwrap();
    assert!(!control.send_color(0, 1, ColorCommand::new([0.0, 1.0, 0.0], 0, false).unwrap()));
    assert!(control.send_color(1, 1, ColorCommand::new([0.0, 1.0, 0.0], 0, false).unwrap()));
    assert!(control.send_color(1, 2, ColorCommand::new([0.0, 0.0, 1.0], 0, false).unwrap()));
    release_tx.send(()).unwrap();
    let mut red_count = 0;
    let mut sent = false;
    loop {
        let (generation, event) = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(generation, 1);
        match event {
            Event::Color {
                state,
                after_write: 0,
            } => {
                assert!(!sent);
                assert_eq!(state.rgb, [1.0, 0.0, 0.0]);
                red_count += 1;
            }
            Event::Sent { revision: 2, color } => {
                assert_eq!(red_count, 2);
                assert_eq!(color.rgb(), [0.0, 0.0, 1.0]);
                sent = true;
            }
            Event::Color {
                state,
                after_write: 2,
            } => {
                assert!(sent);
                assert_eq!(state.rgb, [0.0, 0.0, 1.0]);
                break;
            }
            _ => panic!("Unexpected synchronization event"),
        }
    }
    assert!(control.send_color(1, 3, ColorCommand::new([1.0; 3], 1, false).unwrap()));
    assert!(matches!(
        rx.recv_timeout(WAIT).unwrap(),
        (1, Event::Sent { revision: 3, .. })
    ));
    drop(worker);
    server.join().unwrap();
}

#[test]
fn rgb_fallback_rechecks_slot_and_discards_a_color_read_across_a_slot_change() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let pairing = pairing(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept(&listener);
        let auth = receive(&mut socket);
        reply(&mut socket, &auth);
        let heartbeat = receive(&mut socket);
        reply(&mut socket, &heartbeat);
        for (command, detail) in [
            (
                "SyncColorCircleUIState",
                json!({"ColorSelectionModel":"HLS","CurrentColorIndex":0,
                "IsCurrentColorTransparent":false,"CanExecuteSetterCommand":true,"IsManipulating":false}),
            ),
            (
                "SyncSubViewUIState",
                json!({"CurrentColorRed":4294967295u32,"CurrentColorGreen":0,"CurrentColorBlue":0}),
            ),
            ("SyncColorCircleUIState", json!({"CurrentColorIndex":1})),
            (
                "SyncSubViewUIState",
                json!({"CurrentColorRed":0,"CurrentColorGreen":4294967295u32,"CurrentColorBlue":0}),
            ),
            ("SyncColorCircleUIState", json!({})),
        ] {
            let request = receive(&mut socket);
            assert_eq!(request.command, command);
            socket
                .get_mut()
                .write_all(
                    &Frame {
                        kind: Kind::Success,
                        command: request.command,
                        serial: request.serial,
                        detail: Some(detail),
                        data: vec![],
                    }
                    .encode()
                    .unwrap(),
                )
                .unwrap();
        }
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
    loop {
        match rx.recv_timeout(WAIT).unwrap() {
            (1, Event::ColorUnavailable) => {}
            (
                1,
                Event::Color {
                    state,
                    after_write: 0,
                },
            ) => {
                assert_eq!(state.slot, 1);
                assert_eq!(state.rgb, [0.0, 1.0, 0.0]);
                break;
            }
            _ => panic!("Unconfirmed RGB was published"),
        }
    }
    drop(worker);
    server.join().unwrap();
}
