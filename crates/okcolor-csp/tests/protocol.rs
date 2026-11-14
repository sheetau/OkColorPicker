use okcolor_csp::{
    Error, Pairing,
    frame::{Decoder, Frame, Kind, MAX_FRAME_BYTES},
};
use serde_json::json;

const GOLDEN: &str = "https://companion.clip-studio.com/rc/en-us?s=458ba0757b17e25a83bc6e7a28ef4081a3694352bf17c0f72f3a56d33391a3617811e842";

#[test]
fn pairing_accepts_the_reference_vector_without_exposing_credentials() {
    let pairing = Pairing::parse(GOLDEN).unwrap();
    assert_eq!(format!("{pairing:?}"), "Pairing([redacted])");
    for bad in [
        "",
        "https://companion.clip-studio.com/rc/en?s=zz",
        "https://companion.clip-studio.com/rc/en?s=1",
    ] {
        assert!(matches!(Pairing::parse(bad), Err(Error::InvalidPairing)));
    }
    for bad in [
        GOLDEN.replace("https:", "http:"),
        GOLDEN.replace("clip-studio.com", "clip-studio.com.evil.invalid"),
        GOLDEN.replace("/rc/", "/other/"),
        GOLDEN.replace("https://", "https://user@"),
        format!("{GOLDEN}&s=00"),
        format!("{GOLDEN}#secret"),
    ] {
        assert!(Pairing::parse(&bad).is_err(), "{bad}");
    }
}

#[test]
fn decoded_pairing_fields_reject_invalid_ports_hosts_and_control_characters() {
    for raw in [
        "127.0.0.1\t0\tsecret\tG#1",
        "127.0.0.1\t65536\tsecret\tG#1",
        "127.0.0.1\t-1\tsecret\tG#1",
        "localhost\t1234\tsecret\tG#1",
        "0.0.0.0\t1234\tsecret\tG#1",
        "224.0.0.1\t1234\tsecret\tG#1",
        "127.0.0.1\t1234\t\tG#1",
        "127.0.0.1\t1234\tsecret\t",
        "127.0.0.1\t1234\tsecret\n\tG#1",
        "127.0.0.1\t1234\tsecret\tG#1\textra",
    ] {
        let key = [0x74, 0xb2, 0x92, 0x5b, 0x4a, 0x21, 0xda];
        let hex: String = raw
            .bytes()
            .enumerate()
            .map(|(i, b)| format!("{:02x}", b ^ key[i % 7]))
            .collect();
        let url = format!("https://companion.clip-studio.com/rc/en-us?s={hex}");
        let error = Pairing::parse(&url).unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
}

#[test]
fn every_tcp_split_and_coalesced_frames_preserve_wire_fields() {
    let wire = b"\x06$tcp_remote_command_protocol_version=1.0\x1e$command=SyncColorCircleUIState\x1e$serial=4294967295\x1e$detail={\"CurrentColorIndex\":1}\x1e\0";
    for split in 0..=wire.len() {
        let mut decoder = Decoder::default();
        let mut frames = decoder.feed(&wire[..split]).unwrap();
        frames.extend(decoder.feed(&wire[split..]).unwrap());
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind, Kind::Success);
        assert_eq!(frames[0].serial, u32::MAX);
        assert_eq!(frames[0].detail, Some(json!({"CurrentColorIndex": 1})));
        assert_eq!(frames[0].encode().unwrap(), wire);
    }
    let mut decoder = Decoder::default();
    let mut frames = vec![];
    for byte in wire.iter().chain(wire.iter()) {
        frames.extend(decoder.feed(&[*byte]).unwrap());
    }
    assert_eq!(frames.len(), 2);
    let mut both = wire.to_vec();
    both.extend_from_slice(wire);
    assert_eq!(Decoder::default().feed(&both).unwrap(), frames);
}

#[test]
fn bodies_retain_json_scalars_empty_details_and_binary_tails() {
    let frame = Frame {
        kind: Kind::Command,
        command: "Authenticate".into(),
        serial: 0,
        detail: Some(json!(["G#1", "secret", "rotated"])),
        data: vec![],
    };
    assert!(!format!("{frame:?}").contains("secret"));
    for detail in [
        None,
        Some(json!(false)),
        Some(json!(123.25)),
        Some(json!("a\u{1e}$serial=99\u{b}\0")),
    ] {
        let frame = Frame {
            kind: Kind::Failure,
            command: "Sample".into(),
            serial: 9,
            detail,
            data: b"base64\x1e$detail=x".to_vec(),
        };
        assert_eq!(
            Decoder::default().feed(&frame.encode().unwrap()).unwrap(),
            [frame]
        );
    }
    let heartbeat = Frame {
        kind: Kind::Command,
        command: "TellHeartbeat".into(),
        serial: 1,
        detail: None,
        data: vec![],
    };
    assert_eq!(heartbeat.encode().unwrap(), b"\x01$tcp_remote_command_protocol_version=1.0\x1e$command=TellHeartbeat\x1e$serial=1\x1e$detail=\x1e\0");
}

#[test]
fn malformed_or_unbounded_frames_are_rejected() {
    let valid = "\u{1}$tcp_remote_command_protocol_version=1.0\u{1e}$command=Ping\u{1e}$serial=0\u{1e}$detail={}\u{1e}\0";
    for bad in [
        valid.replace("1.0", "2.0"),
        valid.replace("serial=0", "serial=-1"),
        valid.replace("serial=0", "serial=4294967296"),
        valid.replace("command=Ping", "command="),
        valid.replace("{}", "{bad}"),
        valid.replace("{}", "{}\u{1e}$serial=4"),
        valid.replace("{}", "["),
        valid.replace("\u{1}$", "\u{7}$"),
        valid.replace("{}\u{1e}", "{}"),
    ] {
        assert!(Decoder::default().feed(bad.as_bytes()).is_err());
    }
    let mut decoder = Decoder::default();
    let chunk = vec![b'a'; 32768];
    for _ in 0..MAX_FRAME_BYTES / chunk.len() {
        assert!(decoder.feed(&chunk).unwrap().is_empty());
    }
    assert!(matches!(decoder.feed(b"x"), Err(Error::FrameTooLarge)));
    assert!(matches!(
        Decoder::default().feed(&vec![1; MAX_FRAME_BYTES + 1]),
        Err(Error::FrameTooLarge)
    ));
}
