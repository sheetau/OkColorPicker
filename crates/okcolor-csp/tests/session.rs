use okcolor_csp::{Error, Pairing, Session};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(2);

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
fn receive(reader: &mut BufReader<TcpStream>) -> (String, u32, Option<Value>) {
    let mut bytes = vec![];
    assert!(reader.read_until(0, &mut bytes).unwrap() > 0);
    assert_eq!(&bytes[..2], b"\x01$");
    let text = std::str::from_utf8(&bytes[2..bytes.len() - 2]).unwrap();
    let fields: Vec<_> = text.splitn(4, "\x1e$").collect();
    assert_eq!(fields[0], "tcp_remote_command_protocol_version=1.0");
    (
        fields[1].strip_prefix("command=").unwrap().to_owned(),
        fields[2][7..].parse().unwrap(),
        if fields[3] == "detail=" {
            None
        } else {
            Some(serde_json::from_str(&fields[3][7..]).unwrap())
        },
    )
}
fn wire(kind: u8, command: &str, serial: u32, detail: &str) -> Vec<u8> {
    let mut bytes = vec![kind];
    bytes.extend_from_slice(format!("$tcp_remote_command_protocol_version=1.0\x1e$command={command}\x1e$serial={serial}\x1e$detail={detail}\x1e\0").as_bytes());
    bytes
}
fn accepted(reader: &mut BufReader<TcpStream>, command: &str, serial: u32) {
    reader
        .get_mut()
        .write_all(&wire(6, command, serial, ""))
        .unwrap();
}
fn server(listener: &TcpListener) -> BufReader<TcpStream> {
    let stream = listener.accept().unwrap().0;
    stream.set_read_timeout(Some(WAIT)).unwrap();
    stream.set_write_timeout(Some(WAIT)).unwrap();
    BufReader::new(stream)
}
fn decoded_auth(hex: &str) -> String {
    let key = [0xb6, 0xd5, 0x92, 0xc4, 0xa7, 0x83, 0xe1];
    String::from_utf8(
        hex.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .map(|(i, p)| {
                u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap() ^ key[i % 7]
            })
            .collect(),
    )
    .unwrap()
}
fn login(reader: &mut BufReader<TcpStream>) -> Value {
    let (command, serial, detail) = receive(reader);
    assert_eq!((command.as_str(), serial), ("Authenticate", 0));
    detail.unwrap()
}

#[test]
fn authenticates_acks_interleaved_pushes_sends_rgb_and_reconnects_with_rotated_secret() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = pairing(listener.local_addr().unwrap().port());
    let worker = thread::spawn(move || {
        let mut socket = server(&listener);
        let auth = login(&mut socket);
        assert_eq!(auth[0], "G#1:2026");
        assert_eq!(decoded_auth(auth[1].as_str().unwrap()), "secretpw");
        let rotated = decoded_auth(auth[2].as_str().unwrap());
        assert_eq!(rotated.len(), 8);
        assert_ne!(rotated, "secretpw");
        socket
            .get_mut()
            .write_all(&wire(
                1,
                "SyncColorCircleUIState",
                71,
                "{\"CurrentColorIndex\":0}",
            ))
            .unwrap();
        let mut ack = vec![];
        socket.read_until(0, &mut ack).unwrap();
        assert_eq!(ack, wire(6, "SyncColorCircleUIState", 71, ""));
        for byte in wire(6, "Authenticate", 0, "{\"AuthErrorReason\":\"Unknown\"}") {
            socket.get_mut().write_all(&[byte]).unwrap();
        }
        let (command, serial, detail) = receive(&mut socket);
        assert_eq!((command.as_str(), serial), ("SetCurrentColor", 1));
        assert_eq!(
            detail,
            Some(json!({"RGBColorR":3,"RGBColorG":99,"RGBColorB":254,"IsColorTransparent":false}))
        );
        // A stale reply must not satisfy the current serial.
        accepted(&mut socket, "SetCurrentColor", 0);
        accepted(&mut socket, "SetCurrentColor", 1);
        let (command, serial, detail) = receive(&mut socket);
        assert_eq!(
            (command.as_str(), serial, detail),
            ("TellHeartbeat", 2, None)
        );
        accepted(&mut socket, &command, serial);
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).unwrap(), 0);
        drop(socket);
        let mut socket = server(&listener);
        let auth = login(&mut socket);
        assert_eq!(auth[0], "G#1:2026");
        assert_eq!(
            decoded_auth(auth[1].as_str().unwrap()),
            "{{(([[reconnection request marker]]))}}\r\n"
        );
        assert_eq!(decoded_auth(auth[2].as_str().unwrap()), rotated);
        socket
            .get_mut()
            .write_all(&wire(6, "Authenticate", 0, "[true,false]"))
            .unwrap();
        let (command, serial, _) = receive(&mut socket);
        assert_eq!((command.as_str(), serial), ("TellHeartbeat", 1));
        accepted(&mut socket, &command, serial);
    });
    let mut session = Session::connect(&config, WAIT).unwrap();
    let push = session.poll(WAIT).unwrap().unwrap();
    assert_eq!(push.serial, 71);
    session.set_rgb([3, 99, 254], false, WAIT).unwrap();
    session.heartbeat(WAIT).unwrap();
    let token = session.reconnect_token();
    assert_eq!(format!("{token:?}"), "ReconnectToken([redacted])");
    drop(session);
    let mut session = Session::reconnect(&token, WAIT).unwrap();
    session.heartbeat(WAIT).unwrap();
    worker.join().unwrap();
}

#[test]
fn authentication_requires_an_explicit_success_body_and_matching_command() {
    for (kind, command, detail) in [
        (6, "Authenticate", ""),
        (6, "Authenticate", "{}"),
        (6, "Authenticate", "[false,false]"),
        (
            6,
            "Authenticate",
            "{\"AuthErrorReason\":\"PasswordMismatch\"}",
        ),
        (0x15, "Authenticate", "{\"AuthErrorReason\":\"Unknown\"}"),
        (6, "WrongCommand", "[true,false]"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = pairing(listener.local_addr().unwrap().port());
        let worker = thread::spawn(move || {
            let mut socket = server(&listener);
            login(&mut socket);
            socket
                .get_mut()
                .write_all(&wire(kind, command, 0, detail))
                .unwrap();
        });
        assert!(Session::connect(&config, WAIT).is_err());
        worker.join().unwrap();
    }
}

#[test]
fn a_request_timeout_closes_the_session_instead_of_reusing_a_late_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = pairing(listener.local_addr().unwrap().port());
    let worker = thread::spawn(move || {
        let mut socket = server(&listener);
        login(&mut socket);
        socket
            .get_mut()
            .write_all(&wire(6, "Authenticate", 0, "[true,false]"))
            .unwrap();
        receive(&mut socket);
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).unwrap(), 0);
    });
    let mut session = Session::connect(&config, WAIT).unwrap();
    let began = Instant::now();
    assert!(matches!(
        session.heartbeat(Duration::from_millis(40)),
        Err(Error::Timeout)
    ));
    assert!(began.elapsed() < Duration::from_secs(1));
    assert!(matches!(session.heartbeat(WAIT), Err(Error::Disconnected)));
    worker.join().unwrap();
}

#[test]
fn an_idle_poll_timeout_keeps_partial_frames_for_the_next_poll() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = pairing(listener.local_addr().unwrap().port());
    let (ready, wait_ready) = std::sync::mpsc::channel();
    let (resume, wait_resume) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let mut socket = server(&listener);
        login(&mut socket);
        socket
            .get_mut()
            .write_all(&wire(6, "Authenticate", 0, "[true,false]"))
            .unwrap();
        let push = wire(1, "SyncSubViewUIState", 42, "{\"CurrentColorRed\":101}");
        socket.get_mut().write_all(&push[..12]).unwrap();
        ready.send(()).unwrap();
        wait_resume.recv_timeout(WAIT).unwrap();
        socket.get_mut().write_all(&push[12..]).unwrap();
        let mut ack = vec![];
        socket.read_until(0, &mut ack).unwrap();
        assert_eq!(ack, wire(6, "SyncSubViewUIState", 42, ""));
    });
    let mut session = Session::connect(&config, WAIT).unwrap();
    wait_ready.recv_timeout(WAIT).unwrap();
    assert!(session.poll(Duration::from_millis(20)).unwrap().is_none());
    resume.send(()).unwrap();
    let event = session.poll(WAIT).unwrap().unwrap();
    assert_eq!(event.detail, Some(json!({"CurrentColorRed":101})));
    assert!(matches!(session.poll(WAIT), Err(Error::Disconnected)));
    worker.join().unwrap();
}

#[test]
fn unsolicited_pushes_are_bounded_even_during_authentication() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = pairing(listener.local_addr().unwrap().port());
    let worker = thread::spawn(move || {
        let mut socket = server(&listener);
        login(&mut socket);
        let bytes: Vec<u8> = (0..17)
            .flat_map(|i| wire(1, "SyncSettingUIState", i, "{}"))
            .collect();
        socket.get_mut().write_all(&bytes).unwrap();
        // Keep the peer open until the bounded client rejects this overflow.
        let mut drain = vec![];
        let _ = socket.read_to_end(&mut drain);
    });
    assert!(matches!(
        Session::connect(&config, WAIT),
        Err(Error::QueueFull)
    ));
    worker.join().unwrap();
}
