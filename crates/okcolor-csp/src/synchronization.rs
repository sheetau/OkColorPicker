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
const COLOR_INTERVAL: Duration = Duration::from_millis(25);

struct Pending {
    serial: u32,
    command: &'static str,
    deadline: Instant,
    write: Option<(u64, ColorCommand)>,
}
pub(crate) struct Connection {
    pending: Option<Pending>,
    pending_heartbeat: Option<Pending>,
    session: Session,
    cache: ColorCache,
    remote: Option<ColorState>,
    heartbeat: Instant,
    read_due: Instant,
    need_rgb: bool,
    confirm_rgb: bool,
    after_write: u64,
    must_read: bool,
}
impl Connection {
    pub fn new(session: Session) -> Self {
        Self {
            pending: None,
            pending_heartbeat: None,
            session,
            cache: ColorCache::default(),
            remote: None,
            heartbeat: Instant::now(),
            read_due: Instant::now(),
            need_rgb: false,
            confirm_rgb: false,
            after_write: 0,
            must_read: true,
        }
    }
    pub fn step(&mut self, control: &Control, generation: u64) -> Result<Vec<Event>, Error> {
        let mut events = vec![];
        self.drain(&mut events)?;
        // One color transaction and one heartbeat may be in flight independently.
        // Reading is continuous on this dedicated worker: notifications are ACKed
        // and published even while either request is waiting for a reply.
        let now = Instant::now();
        let mut wait = COLOR_INTERVAL;
        if self.pending.is_none() {
            wait = wait.min(self.read_due.saturating_duration_since(now));
        }
        if self.pending_heartbeat.is_none() {
            wait = wait.min(self.heartbeat.saturating_duration_since(now));
        }
        if let Some(frame) = self.session.receive(wait.max(Duration::from_millis(1)))? {
            if frame.kind == Kind::Command {
                self.absorb(frame, &mut events)?;
            } else if self
                .pending_heartbeat
                .as_ref()
                .is_some_and(|p| p.serial == frame.serial)
            {
                let pending = self.pending_heartbeat.take().expect("matched heartbeat");
                if frame.command != pending.command {
                    return Err(Error::InvalidFrame);
                }
                if frame.kind == Kind::Failure {
                    return Err(Error::CommandRejected);
                }
                self.heartbeat = Instant::now() + Duration::from_secs(1);
            } else if self
                .pending
                .as_ref()
                .is_some_and(|p| p.serial == frame.serial)
            {
                let pending = self.pending.take().expect("matched color transaction");
                if frame.command != pending.command {
                    return Err(Error::InvalidFrame);
                }
                if let Some((revision, color)) = pending.write {
                    if frame.kind == Kind::Success {
                        self.after_write = revision;
                        self.must_read = true;
                        events.push(Event::Sent { revision, color });
                        self.read_due = Instant::now();
                    } else {
                        events.push(Event::SendRejected { revision });
                    }
                } else if frame.kind == Kind::Success {
                    self.must_read = false;
                    self.absorb(frame, &mut events)?;
                } else {
                    self.must_read = false;
                    self.remote = None;
                    self.need_rgb = false;
                    self.confirm_rgb = false;
                    events.push(Event::ColorUnavailable);
                }
            }
        }
        if [&self.pending, &self.pending_heartbeat]
            .into_iter()
            .any(|p| p.as_ref().is_some_and(|p| Instant::now() >= p.deadline))
        {
            return Err(Error::Timeout);
        }
        if self.pending_heartbeat.is_none() && Instant::now() >= self.heartbeat {
            let serial = self.session.start_command(
                "TellHeartbeat",
                Some(json!({"IdleTimerResetRequested":true})),
                RESPONSE,
            )?;
            self.pending_heartbeat = Some(Pending {
                serial,
                command: "TellHeartbeat",
                deadline: Instant::now() + RESPONSE,
                write: None,
            });
            return Ok(events);
        }
        if self.pending.is_none() {
            // Confirm each write before sending another. Never let a read taken
            // before a setter ACK masquerade as confirmation of that setter.
            if !self.need_rgb
                && !self.confirm_rgb
                && !self.must_read
                && let Some((revision, color)) = control.take_color(generation)
            {
                if revision <= self.after_write
                    || !self.remote.is_some_and(|r| r.can_set && !r.manipulating)
                {
                    events.push(Event::SendRejected { revision });
                } else {
                    self.start("SetCurrentColor",Some(json!({
                        "ColorSpaceKind":"HSV","ColorIndex":color.slot(),
                        "HSVColorH":color.hsv[0],"HSVColorS":color.hsv[1],"HSVColorV":color.hsv[2],
                        "IsColorTransparent":color.transparent(),
                    })),Some((revision,color)))?;
                }
            } else if self.need_rgb
                || self.confirm_rgb
                || self.must_read
                || Instant::now() >= self.read_due
            {
                let command = if self.need_rgb {
                    "SyncSubViewUIState"
                } else {
                    "SyncColorCircleUIState"
                };
                self.need_rgb = false;
                self.start(command, Some(json!({})), None)?;
                // Start-to-start cadence: slow replies never create a backlog.
                self.read_due = Instant::now() + COLOR_INTERVAL;
            }
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
    fn absorb(&mut self, mut frame: Frame, events: &mut Vec<Event>) -> Result<(), Error> {
        if frame.command == "PreNotifyOfServerShutdown" {
            return Err(Error::Disconnected);
        }
        // Some CSP panels send the same color fields under another command.
        // Keep the wire ACK's original identity; normalize only the local parser.
        if let Some(detail) = frame.detail.as_ref().and_then(|v| v.as_object()) {
            if detail.keys().any(|key| {
                key.starts_with("HSVColorMain")
                    || key.starts_with("HSVColorSub")
                    || key.starts_with("HLSColorMain")
                    || key.starts_with("HLSColorSub")
            }) {
                frame.command = "SyncColorCircleUIState".into();
            } else if detail.contains_key("CurrentColorRed")
                || detail.contains_key("CurrentColorGreen")
                || detail.contains_key("CurrentColorBlue")
            {
                frame.command = "SyncSubViewUIState".into();
            }
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
