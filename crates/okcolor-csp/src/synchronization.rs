//! Per-socket state and bounded scheduling; no picker state or history lives here.
use crate::{
    Error, Session,
    color::{ColorCache, ColorState},
    frame::Frame,
    worker::{Control, Event},
};
use crate::{color::ColorCommand, frame::Kind};
use serde_json::json;
use std::time::{Duration, Instant};
const RESPONSE: Duration = Duration::from_millis(500);

struct Pending {
    serial: u32,
    command: &'static str,
    deadline: Instant,
    write: Option<(u64, ColorCommand)>,
}
pub(crate) struct Connection {
    pending: Option<Pending>,
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
            pending: None,
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
        // A push becomes an event in this step, even while a heartbeat, read or
        // write is awaiting its ACK. There is at most one outstanding request.
        if self.pending.is_some() {
            if let Some(frame) = self.session.receive(Duration::from_millis(25))? {
                if frame.kind == Kind::Command {
                    self.absorb(frame, &mut events)?;
                } else if self
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.serial == frame.serial)
                {
                    let pending = self.pending.take().expect("matched pending request");
                    if frame.command != pending.command {
                        return Err(Error::InvalidFrame);
                    }
                    if let Some((revision, color)) = pending.write {
                        if frame.kind == Kind::Success {
                            self.after_write = revision;
                            events.push(Event::Sent { revision, color });
                            self.read_due = Instant::now();
                        } else {
                            events.push(Event::SendRejected { revision });
                        }
                    } else if pending.command == "TellHeartbeat" {
                        if frame.kind == Kind::Failure {
                            return Err(Error::CommandRejected);
                        }
                        self.heartbeat = Instant::now() + Duration::from_secs(1);
                    } else {
                        if frame.kind == Kind::Success {
                            self.absorb(frame, &mut events)?;
                        } else {
                            self.remote = None;
                            self.need_rgb = false;
                            self.confirm_rgb = false;
                            events.push(Event::ColorUnavailable);
                        }
                        // Periodic reads only recover missed state; pushed color samples
                        // are acknowledged and published as they arrive.
                        self.read_due = Instant::now() + Duration::from_millis(500);
                    }
                }
            }
            if self
                .pending
                .as_ref()
                .is_some_and(|p| Instant::now() >= p.deadline)
            {
                return Err(Error::Timeout);
            }
        } else if Instant::now() >= self.heartbeat {
            self.start(
                "TellHeartbeat",
                Some(json!({"IdleTimerResetRequested":true})),
                None,
            )?;
        } else if let Some((revision, color)) =
            if !self.need_rgb && !self.confirm_rgb && Instant::now() < self.read_due {
                control.take_color(generation)
            } else {
                None
            }
        {
            if revision <= self.after_write
                || !self
                    .remote
                    .is_some_and(|remote| remote.can_set && !remote.manipulating)
            {
                events.push(Event::SendRejected { revision });
            } else {
                self.start("SetCurrentColor", Some(json!({
                    "ColorSpaceKind":"HSV", "ColorIndex":color.slot(),
                    "HSVColorH":color.hsv[0], "HSVColorS":color.hsv[1], "HSVColorV":color.hsv[2],
                    "IsColorTransparent":color.transparent(),
                })), Some((revision, color)))?;
            }
        } else if self.need_rgb || self.confirm_rgb || Instant::now() >= self.read_due {
            let command = if self.need_rgb {
                "SyncSubViewUIState"
            } else {
                "SyncColorCircleUIState"
            };
            self.need_rgb = false;
            self.start(command, None, None)?;
        } else if let Some(frame) = self.session.receive(Duration::from_millis(25))?
            && frame.kind == Kind::Command
        {
            self.absorb(frame, &mut events)?;
        }
        Ok(events)
    }
    fn start(
        &mut self,
        command: &'static str,
        detail: Option<serde_json::Value>,
        write: Option<(u64, ColorCommand)>,
    ) -> Result<(), Error> {
        let serial = self.session.start_command(command, detail, RESPONSE)?;
        self.pending = Some(Pending {
            serial,
            command,
            deadline: Instant::now() + RESPONSE,
            write,
        });
        Ok(())
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
