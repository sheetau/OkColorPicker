//! UI bridge for a runtime-only CSP connection. Credentials never enter saved state.
use crate::{
    companion_sync::{Inbox, Resolution, Sync},
    model::PickerModel,
};
use okcolor_csp::worker::{Control, Event, State, Worker};
use std::{
    cell::{Cell, RefCell},
    time::Instant,
};
use zgui::{prelude::*, reactive::RenderEffect};

#[derive(Clone, Copy)]
pub struct Companion {
    pub state: RwSignal<State>,
    pub hint: RwSignal<String>,
    pub can_resolve: RwSignal<bool>,
    control: RwSignal<Option<Control>>,
    resolution: RwSignal<(u64, Resolution)>,
}
impl Default for Companion {
    fn default() -> Self {
        Self {
            state: RwSignal::new(State::Idle),
            hint: RwSignal::new(String::new()),
            can_resolve: RwSignal::new(false),
            control: RwSignal::new(None),
            resolution: RwSignal::new((0, Resolution::None)),
        }
    }
}
impl Companion {
    pub fn bind(self, model: PickerModel) {
        self.bind_scanner(model, |cancelled| {
            #[cfg(target_os = "windows")]
            {
                crate::windows_qr::scan(cancelled)
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = cancelled;
                Err(okcolor_csp::qr::ScanError::CaptureUnavailable)
            }
        });
    }
    fn bind_scanner(
        self,
        model: PickerModel,
        scan: impl Fn(&dyn Fn() -> bool) -> Result<okcolor_csp::Pairing, okcolor_csp::qr::ScanError>
        + Send
        + 'static,
    ) {
        let incoming = RwSignal::new(Inbox::default());
        let worker = Worker::start(scan, move |generation, event| {
            if matches!(event, Event::Push(_) | Event::Sent { .. }) {
                return;
            }
            incoming.try_update(|inbox| {
                inbox.absorb(generation, event);
            });
        });
        let worker = match worker {
            Ok(worker) => worker,
            Err(_) => {
                self.state
                    .set(State::Failed("Cannot start CSP connection worker".into()));
                return;
            }
        };
        let control = worker.control();
        self.control.set(Some(control.clone()));
        let sync = RefCell::new(Sync::new(model));
        let resolved = Cell::new(0);
        let effect = RenderEffect::new(move |_| {
            let inbox = incoming.get();
            model.edit.get();
            model.mapping.get();
            model.dragging.get();
            model.editing_text.get();
            let (request, resolution) = self.resolution.get();
            let resolution = if request != resolved.get() {
                resolved.set(request);
                resolution
            } else {
                Resolution::None
            };
            let generation = control.generation();
            if inbox.generation == generation
                && let Some(state) = &inbox.state
                && self.state.get_untracked() != *state
            {
                self.state.set(state.clone());
            }
            let feedback = sync.borrow_mut().tick(
                model,
                &inbox,
                generation,
                resolution,
                Instant::now(),
                |g, r, c| control.send_color(g, r, c),
            );
            if self.hint.get_untracked() != feedback.hint {
                self.hint.set(feedback.hint);
            }
            if self.can_resolve.get_untracked() != feedback.resolve {
                self.can_resolve.set(feedback.resolve);
            }
        });
        on_cleanup_local(move || {
            drop(effect);
            drop(worker);
        });
    }
    pub fn use_csp(self) {
        self.resolve(Resolution::Pull);
    }
    pub fn send_current(self) {
        self.resolve(Resolution::Send);
    }
    fn resolve(self, resolution: Resolution) {
        self.resolution.update(|request| {
            *request = (
                request
                    .0
                    .checked_add(1)
                    .expect("CSP action counter exhausted"),
                resolution,
            )
        });
    }
    pub fn paste_link(self, text: &str) {
        if let Some(control) = self.control.get_untracked() {
            match okcolor_csp::Pairing::parse(text) {
                Ok(pairing) => {
                    control.pair(pairing);
                    self.state.set(State::Connecting);
                    self.hint.set(String::new());
                    self.can_resolve.set(false);
                }
                Err(_) => self.state.set(State::Failed(
                    "Clipboard has no valid CSP pairing link".into(),
                )),
            }
        }
    }
    pub fn scan(self) {
        if let Some(control) = self.control.get_untracked() {
            control.scan();
            self.state.set(State::Scanning);
            self.hint.set(String::new());
            self.can_resolve.set(false);
        }
    }
    pub fn disconnect(self) {
        if let Some(control) = self.control.get_untracked() {
            control.disconnect();
        }
        self.state.set(State::Idle);
        self.hint.set(String::new());
        self.can_resolve.set(false);
    }
    pub fn active(self) -> bool {
        matches!(
            self.state.get(),
            State::Scanning | State::Connecting | State::Connected | State::Reconnecting
        )
    }
    pub fn label(self) -> &'static str {
        match self.state.get() {
            State::Idle | State::Failed(_) => "Connect CSP",
            State::Scanning => "CSP scanning…",
            State::Connecting => "CSP connecting…",
            State::Connected => "CSP connected",
            State::Reconnecting => "CSP reconnecting…",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use okcolor_core::{Gamut, Oklch};
    use okcolor_csp::{
        Pairing,
        frame::{Decoder, Frame, Kind},
    };
    use serde_json::json;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };
    fn receive(socket: &mut BufReader<TcpStream>) -> Frame {
        let mut bytes = vec![];
        assert!(socket.read_until(0, &mut bytes).unwrap() > 0);
        Decoder::default().feed(&bytes).unwrap().pop().unwrap()
    }
    fn respond(
        socket: &mut BufReader<TcpStream>,
        request: &Frame,
        detail: Option<serde_json::Value>,
    ) {
        socket
            .get_mut()
            .write_all(
                &Frame {
                    kind: Kind::Success,
                    command: request.command.clone(),
                    serial: request.serial,
                    detail,
                    data: vec![],
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
    }
    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            zgui::reactive::flush();
            if predicate() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Reactive CSP integration timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn real_worker_and_ui_effect_exchange_colors_without_echo_or_precision_loss() {
        zgui::reactive::install().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let payload = format!(
            "127.0.0.1\t{}\tsecretpw\tG#1:2026",
            listener.local_addr().unwrap().port()
        );
        let key = [0x74, 0xb2, 0x92, 0x5b, 0x4a, 0x21, 0xda];
        let encoded: String = payload
            .bytes()
            .enumerate()
            .map(|(i, b)| format!("{:02x}", b ^ key[i % 7]))
            .collect();
        let pairing = Pairing::parse(&format!(
            "https://companion.clip-studio.com/rc/en-us?s={encoded}"
        ))
        .unwrap();
        let server = thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(4);
            let socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("Mock CSP accept: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            let mut socket = BufReader::new(socket);
            let auth = receive(&mut socket);
            assert_eq!(auth.command, "Authenticate");
            respond(&mut socket, &auth, Some(json!([true, false])));
            let heartbeat = receive(&mut socket);
            assert_eq!(heartbeat.command, "TellHeartbeat");
            respond(&mut socket, &heartbeat, None);
            let first = receive(&mut socket);
            assert_eq!(first.command, "SyncColorCircleUIState");
            let mut state = json!({"CurrentColorIndex":0,"ColorSelectionModel":"HSV",
                "HSVColorMainH":0,"HSVColorMainS":4294967295u32,"HSVColorMainV":4294967295u32,
                "IsCurrentColorTransparent":false,"CanExecuteSetterCommand":true,"IsManipulating":false});
            respond(&mut socket, &first, Some(state.clone()));
            let set = receive(&mut socket);
            assert_eq!(set.command, "SetCurrentColor");
            let fields = set.detail.unwrap();
            assert_eq!(fields.as_object().unwrap().len(), 6);
            assert_eq!(fields["ColorIndex"], 0);
            assert_eq!(fields["ColorSpaceKind"], "HSV");
            assert_ne!(
                fields["HSVColorH"], 0,
                "The received red must not echo before the local edit"
            );
            for suffix in ["H", "S", "V"] {
                state[format!("HSVColorMain{suffix}")] =
                    fields[format!("HSVColor{suffix}")].clone();
            }
            let ack = Frame {
                detail: None,
                ..set
            };
            respond(&mut socket, &ack, None);
            let poll = receive(&mut socket);
            assert_eq!(poll.command, "SyncColorCircleUIState");
            respond(&mut socket, &poll, Some(state));
            while let Ok(count) = socket.read(&mut [0; 4096]) {
                if count == 0 {
                    break;
                }
            }
        });
        let owner = Owner::new();
        owner.with(|| {
            let model =
                PickerModel::new(Oklch::from_rgb([0.2; 3], Gamut::Srgb, 0.314159, 90.0).unwrap());
            let before = model.current();
            model
                .companion
                .bind_scanner(model, move |_| Ok(pairing.clone()));
            model.companion.scan();
            wait_until(|| model.edit.get_untracked().origin == crate::model::EditOrigin::Companion);
            assert_eq!(model.current().components()[3], before.components()[3]);
            assert_eq!(
                model.slots[1].get_untracked(),
                crate::persistence::SavedState::default().sub
            );
            let source = Oklch::new(
                0.6543210987654321,
                0.3210987654321098,
                123.4567890123456,
                0.3456789012345678,
            )
            .unwrap();
            model.select(source);
            wait_until(|| model.companion.hint.get_untracked().starts_with("Sending"));
            wait_until(|| {
                model
                    .companion
                    .hint
                    .get_untracked()
                    .contains("Alpha stays local")
            });
            assert_eq!(model.current(), source);
            assert_eq!(
                model.edit.get_untracked().origin,
                crate::model::EditOrigin::Local
            );
            assert_eq!(model.recent.get_untracked().len(), 2);
        });
        drop(owner);
        server.join().unwrap();
        zgui::reactive::shutdown();
    }
}
