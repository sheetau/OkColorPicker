use crate::{
    Error, Pairing,
    frame::{Decoder, Frame, Kind},
    pairing::{auth_hex, fresh_password},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fmt,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    time::{Duration, Instant},
};

const RECONNECT_MARKER: &str = "{{(([[reconnection request marker]]))}}\r\n";

/// Created only after successful authentication. Keep in memory, never in logs.
#[derive(Clone)]
pub struct ReconnectToken {
    endpoint: SocketAddr,
    generation: String,
    password: String,
}
impl fmt::Debug for ReconnectToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReconnectToken([redacted])")
    }
}

/// Blocking socket owner for a background worker. Every operation has a deadline.
/// Send `heartbeat` once per second while idle and drain pushes through `poll`.
/// A transport/protocol error invalidates this connection; reconnect explicitly.
pub struct Session {
    stream: TcpStream,
    decoder: Decoder,
    packets: VecDeque<Frame>,
    events: VecDeque<Frame>,
    serial: u32,
    token: ReconnectToken,
    usable: bool,
}
impl Session {
    pub fn connect(pairing: &Pairing, timeout: Duration) -> Result<Self, Error> {
        let deadline = deadline(timeout)?;
        let password = fresh_password()?;
        let mut session = Self::open(
            &pairing.endpoints,
            pairing.generation.clone(),
            password,
            deadline,
        )?;
        let detail = json!([
            pairing.generation,
            auth_hex(&pairing.password),
            auth_hex(&session.token.password)
        ]);
        session.authenticate(detail, deadline)?;
        Ok(session)
    }
    pub fn reconnect(token: &ReconnectToken, timeout: Duration) -> Result<Self, Error> {
        let deadline = deadline(timeout)?;
        let mut session = Self::open(
            &[token.endpoint],
            token.generation.clone(),
            token.password.clone(),
            deadline,
        )?;
        let detail = json!([
            token.generation,
            auth_hex(RECONNECT_MARKER),
            auth_hex(&token.password)
        ]);
        session.authenticate(detail, deadline)?;
        Ok(session)
    }
    pub fn reconnect_token(&self) -> ReconnectToken {
        self.token.clone()
    }

    fn open(
        endpoints: &[SocketAddr],
        generation: String,
        password: String,
        deadline: Instant,
    ) -> Result<Self, Error> {
        let mut last = Error::Disconnected;
        for &endpoint in endpoints {
            let remaining = remaining(deadline)?;
            // Do not let one unreachable interface consume the entire dial budget.
            match TcpStream::connect_timeout(&endpoint, remaining.min(Duration::from_millis(750))) {
                Ok(stream) => {
                    stream.set_nodelay(true)?;
                    return Ok(Self {
                        stream,
                        decoder: Decoder::default(),
                        packets: VecDeque::new(),
                        events: VecDeque::new(),
                        serial: 0,
                        token: ReconnectToken {
                            endpoint,
                            generation,
                            password,
                        },
                        usable: true,
                    });
                }
                Err(error) => last = error.into(),
            }
        }
        Err(last)
    }
    fn authenticate(&mut self, detail: Value, deadline: Instant) -> Result<(), Error> {
        let reply = self.exchange("Authenticate", Some(detail), deadline)?;
        let accepted = reply.kind == Kind::Success
            && reply.data.is_empty()
            && match reply.detail {
                Some(Value::Object(object)) => matches!(
                    object.get("AuthErrorReason").and_then(Value::as_str),
                    Some("" | "Unknown")
                ),
                Some(Value::Array(values)) => {
                    values.len() == 2 && values[0] == true && values[1].is_boolean()
                }
                _ => false,
            };
        if accepted {
            Ok(())
        } else {
            self.close();
            Err(Error::AuthenticationRejected)
        }
    }

    pub fn command(
        &mut self,
        command: &str,
        detail: Option<Value>,
        timeout: Duration,
    ) -> Result<Frame, Error> {
        // Authentication has a distinct lifecycle and may only happen at construction.
        if command == "Authenticate" {
            return Err(Error::InvalidFrame);
        }
        let reply = self.exchange(command, detail, deadline(timeout)?)?;
        if reply.kind == Kind::Failure {
            Err(Error::CommandRejected)
        } else {
            Ok(reply)
        }
    }
    pub fn heartbeat(&mut self, timeout: Duration) -> Result<(), Error> {
        self.command("TellHeartbeat", None, timeout).map(|_| ())
    }
    /// Active heartbeat opens CSP's state synchronization gate.
    pub fn active_heartbeat(&mut self, timeout: Duration) -> Result<(), Error> {
        self.command(
            "TellHeartbeat",
            Some(json!({"IdleTimerResetRequested":true})),
            timeout,
        )
        .map(|_| ())
    }
    pub fn set_color(
        &mut self,
        color: crate::color::ColorCommand,
        timeout: Duration,
    ) -> Result<(), Error> {
        self.command(
            "SetCurrentColor",
            Some(json!({
                "ColorSpaceKind":"HSV", "ColorIndex":color.slot,
                "HSVColorH":color.hsv[0], "HSVColorS":color.hsv[1], "HSVColorV":color.hsv[2],
                "IsColorTransparent":color.transparent,
            })),
            timeout,
        )
        .map(|_| ())
    }
    /// CSP accepts bounded RGB plus a transparent-color selector, not continuous alpha.
    pub fn set_rgb(
        &mut self,
        rgb: [u8; 3],
        transparent: bool,
        timeout: Duration,
    ) -> Result<(), Error> {
        self.command(
            "SetCurrentColor",
            Some(json!({
                "RGBColorR": rgb[0], "RGBColorG": rgb[1], "RGBColorB": rgb[2],
                "IsColorTransparent": transparent,
            })),
            timeout,
        )
        .map(|_| ())
    }
    pub fn poll(&mut self, timeout: Duration) -> Result<Option<Frame>, Error> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        let deadline = deadline(timeout)?;
        loop {
            match self.read_packet(deadline) {
                Ok(frame) if frame.kind == Kind::Command => {
                    if let Err(error) = self.acknowledge(&frame, deadline) {
                        self.close();
                        return Err(error);
                    }
                    return Ok(Some(frame));
                }
                Ok(_) => {} // A late response cannot satisfy a different request.
                Err(Error::Timeout) => return Ok(None),
                Err(error) => {
                    self.close();
                    return Err(error);
                }
            }
        }
    }
    pub(crate) fn queued_event(&mut self) -> Option<Frame> {
        self.events.pop_front()
    }
    /// Start one request without holding later host pushes behind its response.
    pub(crate) fn start_command(
        &mut self,
        command: &str,
        detail: Option<Value>,
        timeout: Duration,
    ) -> Result<u32, Error> {
        if !self.usable || command == "Authenticate" {
            return Err(Error::InvalidFrame);
        }
        let serial = self.serial;
        self.serial = self.serial.checked_add(1).ok_or(Error::SerialExhausted)?;
        self.write(
            &Frame {
                kind: Kind::Command,
                command: command.into(),
                serial,
                detail,
                data: vec![],
            },
            deadline(timeout)?,
        )?;
        Ok(serial)
    }
    /// Return replies and acknowledged host pushes in wire order.
    pub(crate) fn receive(&mut self, timeout: Duration) -> Result<Option<Frame>, Error> {
        if let Some(frame) = self.events.pop_front() {
            return Ok(Some(frame));
        }
        let read_deadline = deadline(timeout)?;
        match self.read_packet(read_deadline) {
            Ok(frame) => {
                if frame.kind == Kind::Command {
                    // A packet arriving at the end of the read wait still gets a
                    // complete, bounded write opportunity for its immediate ACK.
                    self.acknowledge(&frame, deadline(Duration::from_millis(500))?)?;
                }
                Ok(Some(frame))
            }
            Err(Error::Timeout) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn exchange(
        &mut self,
        command: &str,
        detail: Option<Value>,
        deadline: Instant,
    ) -> Result<Frame, Error> {
        let result = (|| {
            if !self.usable {
                return Err(Error::Disconnected);
            }
            let serial = self.serial;
            self.serial = self.serial.checked_add(1).ok_or(Error::SerialExhausted)?;
            self.write(
                &Frame {
                    kind: Kind::Command,
                    command: command.into(),
                    serial,
                    detail,
                    data: vec![],
                },
                deadline,
            )?;
            loop {
                let frame = self.read_packet(deadline)?;
                if frame.kind == Kind::Command {
                    if self.events.len() == 16 {
                        return Err(Error::QueueFull);
                    }
                    self.acknowledge(&frame, deadline)?;
                    self.events.push_back(frame);
                } else if frame.serial == serial {
                    if frame.command != command {
                        return Err(Error::InvalidFrame);
                    }
                    return Ok(frame);
                }
            }
        })();
        if result.is_err() {
            self.close();
        }
        result
    }
    fn acknowledge(&mut self, frame: &Frame, deadline: Instant) -> Result<(), Error> {
        self.write(
            &Frame {
                kind: Kind::Success,
                command: frame.command.clone(),
                serial: frame.serial,
                detail: None,
                data: vec![],
            },
            deadline,
        )
    }
    fn write(&mut self, frame: &Frame, deadline: Instant) -> Result<(), Error> {
        let bytes = frame.encode()?;
        let mut pending = bytes.as_slice();
        while !pending.is_empty() {
            self.stream.set_write_timeout(Some(remaining(deadline)?))?;
            match self.stream.write(pending) {
                Ok(0) => return Err(Error::Disconnected),
                Ok(written) => pending = &pending[written..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn read_packet(&mut self, deadline: Instant) -> Result<Frame, Error> {
        if !self.usable {
            return Err(Error::Disconnected);
        }
        loop {
            let remaining = remaining(deadline)?;
            if let Some(packet) = self.packets.pop_front() {
                return Ok(packet);
            }
            self.stream.set_read_timeout(Some(remaining))?;
            let mut bytes = [0; 4096];
            let count = match self.stream.read(&mut bytes) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                return Err(Error::Disconnected);
            }
            self.packets.extend(self.decoder.feed(&bytes[..count])?);
        }
    }
    fn close(&mut self) {
        self.usable = false;
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
fn deadline(timeout: Duration) -> Result<Instant, Error> {
    Instant::now().checked_add(timeout).ok_or(Error::Timeout)
}
fn remaining(deadline: Instant) -> Result<Duration, Error> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or(Error::Timeout)
}
