//! Bounded CSP companion protocol, independent of the picker and native UI.
pub mod color;
pub mod frame;
mod pairing;
pub mod qr;
mod session;
mod synchronization;
pub mod worker;
pub use pairing::Pairing;
pub use session::{ReconnectToken, Session};
use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    InvalidPairing,
    InvalidFrame,
    InvalidColorState,
    FrameTooLarge,
    AuthenticationRejected,
    CommandRejected,
    Timeout,
    Disconnected,
    QueueFull,
    SerialExhausted,
    RandomUnavailable,
    Io(io::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidPairing => "Invalid CSP companion pairing URL",
            Self::InvalidFrame => "Invalid CSP protocol frame",
            Self::InvalidColorState => "Invalid or unsupported CSP color state",
            Self::FrameTooLarge => "CSP frame exceeds the size limit",
            Self::AuthenticationRejected => "CSP authentication rejected; scan a new pairing QR",
            Self::CommandRejected => "CSP rejected the command",
            Self::Timeout => "CSP response timed out",
            Self::Disconnected => "CSP connection closed",
            Self::QueueFull => "CSP event queue is full",
            Self::SerialExhausted => "CSP command serials exhausted; reconnect",
            Self::RandomUnavailable => "Cannot generate a CSP reconnect password",
            Self::Io(error) => return write!(f, "CSP socket error: {error}"),
        };
        f.write_str(message)
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            Self::Timeout
        } else {
            Self::Io(error)
        }
    }
}
