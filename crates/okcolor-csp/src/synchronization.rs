//! Per-socket state and bounded scheduling; no picker state or history lives here.
use crate::{
    Error, Session,
    color::{ColorCache, ColorState},
    frame::Frame,
    worker::{Control, Event},
};
use std::time::{Duration, Instant};
const RESPONSE: Duration = Duration::from_millis(500);

pub(crate) struct Connection {
    session: Session,
    cache: ColorCache,
    remote: Option<ColorState>,
    heartbeat: Instant,
    read_due: Instant,
    need_rgb: bool,
    confirm_rgb: bool,
    after_write: u64,
}
impl Connection {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            cache: ColorCache::default(),
            remote: None,
            heartbeat: Instant::now(),
            read_due: Instant::now(),
            need_rgb: false,
            confirm_rgb: false,
            after_write: 0,
        }
    }
    pub fn step(&mut self, control: &Control, generation: u64) -> Result<Vec<Event>, Error> {
        let mut events = vec![];
        // Process queued pushes before any later request/response observation.
        self.drain(&mut events)?;
        if Instant::now() >= self.heartbeat {
            self.session.active_heartbeat(RESPONSE)?;
            self.drain(&mut events)?;
            self.heartbeat = Instant::now() + Duration::from_secs(1);
        } else if let Some((revision, color)) =
            if !self.need_rgb && !self.confirm_rgb && Instant::now() < self.read_due {
                control.take_color(generation)
            } else {
                None
            }
        {
            if revision <= self.after_write
                || !self.remote.is_some_and(|remote| {
                    remote.can_set && !remote.manipulating && remote.slot == color.slot()
                })
            {
                events.push(Event::SendRejected { revision });
            } else {
                let result = self.session.set_color(color, RESPONSE);
                self.drain(&mut events)?;
                match result {
                    Ok(()) => {
                        self.after_write = revision;
                        events.push(Event::Sent { revision, color });
                        self.read_due = Instant::now();
                    }
                    Err(Error::CommandRejected) => events.push(Event::SendRejected { revision }),
                    Err(error) => return Err(error),
                }
            }
        } else if self.need_rgb || self.confirm_rgb || Instant::now() >= self.read_due {
            let command = if self.need_rgb {
                "SyncSubViewUIState"
            } else {
                "SyncColorCircleUIState"
            };
            self.need_rgb = false;
            let reply = self.session.command(command, None, RESPONSE);
            self.drain(&mut events)?;
            match reply {
                Ok(frame) => self.absorb(frame, &mut events)?,
                Err(Error::CommandRejected) => {
                    self.remote = None;
                    self.need_rgb = false;
                    self.confirm_rgb = false;
                    events.push(Event::ColorUnavailable);
                }
                Err(error) => return Err(error),
            }
            self.read_due = Instant::now() + Duration::from_millis(500);
        } else if let Some(frame) = self.session.poll(Duration::from_millis(25))? {
            self.absorb(frame, &mut events)?;
        }
        Ok(events)
    }
    fn drain(&mut self, events: &mut Vec<Event>) -> Result<(), Error> {
        while let Some(frame) = self.session.queued_event() {
            self.absorb(frame, events)?;
        }
        Ok(())
    }
    fn absorb(&mut self, frame: Frame, events: &mut Vec<Event>) -> Result<(), Error> {
        if frame.command == "PreNotifyOfServerShutdown" {
            return Err(Error::Disconnected);
        }
        let is_color = matches!(
            frame.command.as_str(),
            "SyncColorCircleUIState" | "SyncSubViewUIState"
        );
        if is_color {
            if frame
                .detail
                .as_ref()
                .is_some_and(|detail| detail.get("ServerSelectedTabKind").is_some())
            {
                self.remote = None;
                self.need_rgb = false;
                self.confirm_rgb = false;
                events.push(Event::ColorUnavailable);
                return Ok(());
            }
            let mut observed = self.cache.apply(&frame)?;
            if frame.command == "SyncSubViewUIState" {
                // RGB has no slot field. Re-read circle metadata before associating
                // it with main/sub; a concurrent slot change invalidates this RGB.
                self.confirm_rgb = observed.is_some();
                self.remote = None;
                events.push(Event::ColorUnavailable);
                return Ok(());
            }
            if self.confirm_rgb && frame.kind == crate::frame::Kind::Success {
                observed = observed.or_else(|| self.cache.rgb_state());
                self.confirm_rgb = false;
            }
            match observed {
                Some(state) => {
                    self.need_rgb = false;
                    self.remote = Some(state);
                    // Poll confirmations must still be delivered when RGB is unchanged.
                    events.push(Event::Color {
                        state,
                        after_write: self.after_write,
                    });
                }
                None => {
                    self.remote = None;
                    self.need_rgb = frame.command == "SyncColorCircleUIState";
                    events.push(Event::ColorUnavailable);
                }
            }
        } else {
            events.push(Event::Push(frame));
        }
        Ok(())
    }
}
