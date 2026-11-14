//! A single background owner for scanning, authentication, heartbeats and pushes.
use crate::{Error, Pairing, ReconnectToken, Session, frame::Frame, qr::ScanError};
use std::{
    io,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Scanning,
    Connecting,
    Connected,
    Reconnecting,
    Failed(String),
}
pub enum Event {
    State(State),
    Push(Frame),
}
enum Request {
    Scan,
    Pair(Pairing),
    Disconnect,
}
#[derive(Default)]
struct Pending {
    generation: u64,
    request: Option<Request>,
    stopping: bool,
}
#[derive(Clone)]
pub struct Control(Arc<(Mutex<Pending>, Condvar)>);
impl Control {
    pub fn scan(&self) {
        self.request(Request::Scan);
    }
    pub fn pair(&self, pairing: Pairing) {
        self.request(Request::Pair(pairing));
    }
    pub fn disconnect(&self) {
        self.request(Request::Disconnect);
    }
    pub fn generation(&self) -> u64 {
        self.0.0.lock().unwrap().generation
    }
    fn request(&self, request: Request) {
        let mut pending = self.0.0.lock().unwrap();
        if pending.stopping {
            return;
        }
        let Some(next) = pending.generation.checked_add(1) else {
            return;
        };
        pending.generation = next;
        pending.request = Some(request);
        self.0.1.notify_one();
    }
    fn cancelled(&self, generation: u64) -> bool {
        let pending = self.0.0.lock().unwrap();
        pending.stopping || pending.generation != generation
    }
    /// Retire queued events from a lost connection without superseding a newer
    /// owner request. Reconnection uses this new generation even before dialing.
    fn retire(&self, generation: u64) -> Option<u64> {
        let mut pending = self.0.0.lock().unwrap();
        if pending.stopping || pending.generation != generation {
            return None;
        }
        let next = generation.checked_add(1)?;
        pending.generation = next;
        Some(next)
    }
    fn wait(&self, duration: Option<Duration>) -> Option<(u64, Request)> {
        let mut pending = self.0.0.lock().unwrap();
        if pending.request.is_none() && !pending.stopping {
            pending = match duration {
                Some(duration) => self.0.1.wait_timeout(pending, duration).unwrap().0,
                None => self
                    .0
                    .1
                    .wait_while(pending, |p| p.request.is_none() && !p.stopping)
                    .unwrap(),
            };
        }
        pending
            .request
            .take()
            .map(|request| (pending.generation, request))
    }
}

pub struct Worker {
    control: Control,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(
        scan: impl Fn(&dyn Fn() -> bool) -> Result<Pairing, ScanError> + Send + 'static,
        report: impl Fn(u64, Event) + Send + 'static,
    ) -> io::Result<Self> {
        let control = Control(Arc::new((Mutex::new(Pending::default()), Condvar::new())));
        let shared = control.clone();
        let thread = thread::Builder::new()
            .name("csp-companion".into())
            .spawn(move || run(shared, scan, report))?;
        Ok(Self {
            control,
            thread: Some(thread),
        })
    }
    pub fn control(&self) -> Control {
        self.control.clone()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        {
            let mut pending = self.control.0.0.lock().unwrap();
            pending.stopping = true;
            pending.request = None;
        }
        self.control.0.1.notify_one();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            eprintln!("CSP worker stopped unexpectedly");
        }
    }
}

fn run(
    control: Control,
    scan: impl Fn(&dyn Fn() -> bool) -> Result<Pairing, ScanError>,
    report: impl Fn(u64, Event),
) {
    let mut generation = 0;
    let mut session: Option<Session> = None;
    let mut token: Option<ReconnectToken> = None;
    let mut heartbeat = Instant::now();
    let mut retry = Instant::now();
    let mut failures = 0u32;
    let publish = |generation, event| {
        if !control.cancelled(generation) {
            report(generation, event);
        }
    };
    loop {
        let delay = if session.is_some() {
            Some(Duration::ZERO)
        } else {
            token
                .as_ref()
                .map(|_| retry.saturating_duration_since(Instant::now()))
        };
        let request = control.wait(delay);
        if control.0.0.lock().unwrap().stopping {
            break;
        }
        if let Some((next, request)) = request {
            generation = next;
            session = None;
            token = None;
            failures = 0;
            let pairing = match request {
                Request::Disconnect => {
                    publish(generation, Event::State(State::Idle));
                    continue;
                }
                Request::Pair(pairing) => Ok(pairing),
                Request::Scan => {
                    publish(generation, Event::State(State::Scanning));
                    scan(&|| control.cancelled(generation))
                }
            };
            if control.cancelled(generation) {
                continue;
            }
            let pairing = match pairing {
                Ok(pairing) => pairing,
                Err(error) => {
                    publish(generation, Event::State(State::Failed(error.to_string())));
                    continue;
                }
            };
            publish(generation, Event::State(State::Connecting));
            let result = Session::connect(&pairing, Duration::from_secs(2));
            if control.cancelled(generation) {
                continue;
            }
            match result {
                Ok(connected) => {
                    token = Some(connected.reconnect_token());
                    session = Some(connected);
                    heartbeat = Instant::now() + Duration::from_secs(1);
                    publish(generation, Event::State(State::Connected));
                }
                Err(error) => publish(generation, Event::State(State::Failed(error.to_string()))),
            }
        }
        if control.cancelled(generation) {
            continue;
        }
        if session.is_none()
            && let Some(reconnect) = &token
        {
            if Instant::now() < retry {
                continue;
            }
            let result = Session::reconnect(reconnect, Duration::from_secs(1));
            if control.cancelled(generation) {
                continue;
            }
            match result {
                Ok(connected) => {
                    session = Some(connected);
                    failures = 0;
                    heartbeat = Instant::now() + Duration::from_secs(1);
                    publish(generation, Event::State(State::Connected));
                }
                Err(Error::AuthenticationRejected | Error::CommandRejected) => {
                    token = None;
                    publish(
                        generation,
                        Event::State(State::Failed(Error::AuthenticationRejected.to_string())),
                    );
                }
                Err(_) => {
                    failures = failures.saturating_add(1);
                    retry = Instant::now() + backoff(failures);
                }
            }
        }
        let Some(connected) = session.as_mut() else {
            continue;
        };
        let result = if Instant::now() >= heartbeat {
            heartbeat = Instant::now() + Duration::from_secs(1);
            connected
                .heartbeat(Duration::from_millis(500))
                .map(|_| None)
        } else {
            connected.poll(Duration::from_millis(50))
        };
        match result {
            Ok(Some(frame)) => {
                let shutdown = frame.command == "PreNotifyOfServerShutdown";
                publish(generation, Event::Push(frame));
                if shutdown {
                    session = None;
                    retry = Instant::now() + backoff(0);
                    if let Some(next) = control.retire(generation) {
                        generation = next;
                        publish(generation, Event::State(State::Reconnecting));
                    }
                }
            }
            Ok(None) => {}
            Err(_) => {
                session = None;
                retry = Instant::now() + backoff(0);
                if let Some(next) = control.retire(generation) {
                    generation = next;
                    publish(generation, Event::State(State::Reconnecting));
                }
            }
        }
    }
}
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(match failures {
        0 => 1,
        1 => 2,
        _ => 5,
    })
}
