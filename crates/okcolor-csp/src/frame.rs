use crate::Error;
use serde_json::Value;
use std::fmt;

pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
const HEADER: &str = "$tcp_remote_command_protocol_version=1.0\x1e$command=";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Command = 1,
    Success = 6,
    Failure = 0x15,
}

#[derive(Clone, PartialEq)]
pub struct Frame {
    pub kind: Kind,
    pub command: String,
    pub serial: u32,
    pub detail: Option<Value>,
    pub data: Vec<u8>,
}
impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("kind", &self.kind)
            .field("command", &self.command)
            .field("serial", &self.serial)
            .field("body", &"[redacted]")
            .finish()
    }
}
impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        if !command_name(&self.command) {
            return Err(Error::InvalidFrame);
        }
        let mut bytes = vec![self.kind as u8];
        bytes.extend_from_slice(
            format!(
                "{HEADER}{}\x1e$serial={}\x1e$detail=",
                self.command, self.serial
            )
            .as_bytes(),
        );
        if let Some(detail) = &self.detail {
            serde_json::to_writer(&mut bytes, detail).map_err(|_| Error::InvalidFrame)?;
        }
        if !self.data.is_empty() {
            if self.data.contains(&0) {
                return Err(Error::InvalidFrame);
            }
            bytes.push(0x0b);
            bytes.extend_from_slice(&self.data);
        }
        bytes.extend_from_slice(b"\x1e\0");
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(Error::FrameTooLarge);
        }
        Ok(bytes)
    }
    fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(Error::FrameTooLarge);
        }
        let (&kind, rest) = bytes.split_first().ok_or(Error::InvalidFrame)?;
        let kind = match kind {
            1 => Kind::Command,
            6 => Kind::Success,
            0x15 => Kind::Failure,
            _ => return Err(Error::InvalidFrame),
        };
        let rest = rest
            .strip_prefix(HEADER.as_bytes())
            .and_then(|s| s.strip_suffix(b"\x1e\0"))
            .ok_or(Error::InvalidFrame)?;
        let separator = |data: &[u8], needle: &[u8]| {
            data.windows(needle.len())
                .position(|w| w == needle)
                .ok_or(Error::InvalidFrame)
        };
        let command_end = separator(rest, b"\x1e$serial=")?;
        let command = std::str::from_utf8(&rest[..command_end]).map_err(|_| Error::InvalidFrame)?;
        if !command_name(command) {
            return Err(Error::InvalidFrame);
        }
        let rest = &rest[command_end + 9..];
        let serial_end = separator(rest, b"\x1e$detail=")?;
        let serial = std::str::from_utf8(&rest[..serial_end]).map_err(|_| Error::InvalidFrame)?;
        if serial.is_empty() || !serial.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::InvalidFrame);
        }
        let serial = serial.parse().map_err(|_| Error::InvalidFrame)?;
        let rest = &rest[serial_end + 9..];
        let detail_end = rest.iter().position(|b| *b == 0x0b).unwrap_or(rest.len());
        let detail = if detail_end == 0 {
            None
        } else {
            Some(serde_json::from_slice(&rest[..detail_end]).map_err(|_| Error::InvalidFrame)?)
        };
        let data = if detail_end < rest.len() {
            rest[detail_end + 1..].to_vec()
        } else {
            vec![]
        };
        Ok(Self {
            kind,
            command: command.to_owned(),
            serial,
            detail,
            data,
        })
    }
}
fn command_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// TCP fragments may end anywhere; only a NUL terminates a protocol frame.
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
}
impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Frame>, Error> {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(Error::FrameTooLarge);
        }
        let mut frames = Vec::new();
        for part in bytes.split_inclusive(|b| *b == 0) {
            if self.pending.len() + part.len() > MAX_FRAME_BYTES {
                return Err(Error::FrameTooLarge);
            }
            self.pending.extend_from_slice(part);
            if part.last() == Some(&0) {
                if frames.len() == 64 {
                    return Err(Error::QueueFull);
                }
                frames.push(Frame::parse(&self.pending)?);
                self.pending.clear();
            }
        }
        Ok(frames)
    }
}
