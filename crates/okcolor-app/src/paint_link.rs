//! Local paint application connections. Only an explicit click starts a worker.
mod adobe;
use crate::{model::PickerModel, workspace::PaintApp};
use okcolor_core::{Gamut, GamutMapping, Oklch};
use okcolor_csp::worker::State;
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{
    Message, WebSocket,
    handshake::server::{ErrorResponse, Request, Response},
    protocol::WebSocketConfig,
};
use zgui::{prelude::*, reactive::RenderEffect};

type Rgb = [u8; 3];
#[derive(Clone, Copy, Default, PartialEq)]
struct Colors {
    slots: [Rgb; 2],
    active: usize,
}

fn rgb_bytes(color: Oklch, mapping: GamutMapping) -> Rgb {
    color
        .mapped_rgb(Gamut::Srgb, mapping)
        .expect("validated picker color")
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}
impl Colors {
    fn read(model: PickerModel, app: PaintApp) -> Self {
        let (colors, slot) = if app.adobe() {
            (model.committed.get(), model.selected.get())
        } else {
            let edit = model.edit.get();
            (edit.colors, edit.slot)
        };
        let mapping = model.mapping.get();
        let slots = colors.map(|color| rgb_bytes(color, mapping));
        Self {
            slots,
            active: usize::from(slot),
        }
    }
}

#[derive(Clone)]
struct Incoming {
    generation: u64,
    state: State,
    slots: [Option<(u64, Rgb)>; 2],
    serial: u64,
    active: usize,
}
impl Incoming {
    fn new(generation: u64, state: State) -> Self {
        Self {
            generation,
            state,
            slots: [None; 2],
            serial: 0,
            active: 0,
        }
    }
}

struct PaletteExport {
    name: String,
    colors: Vec<Rgb>,
}

enum Command {
    Select(u64, Option<PaintApp>),
    Palette(u64, PaletteExport),
}
#[derive(Clone, Copy)]
pub(crate) struct PaintLink {
    model: PickerModel,
    channel: StoredValue<Option<mpsc::Sender<Command>>>,
    colors: StoredValue<Arc<Mutex<Colors>>>,
    generation: RwSignal<u64>,
    inbox: RwSignal<Incoming>,
}
impl PaintLink {
    pub fn mount(model: PickerModel) -> Self {
        let link = Self {
            model,
            channel: StoredValue::new(None),
            colors: StoredValue::new(Arc::default()),
            generation: RwSignal::new(0),
            inbox: RwSignal::new(Incoming::new(0, State::Idle)),
        };
        let effect = RenderEffect::new(move |last: Option<(u64, u64)>| {
            let incoming = link.inbox.get();
            if matches!(incoming.state, State::Idle | State::Failed(_)) {
                return (incoming.generation, 0);
            }
            let app = model.tools.with(|t| t.paint_app);
            if app.adobe() && model.stepping_channel.get() {
                return (incoming.generation, 0);
            }
            let Colors { slots, active } = Colors::read(model, app);
            link.colors
                .with_value(|c| *c.lock().expect("paint colors") = Colors { slots, active });
            let generation = link.generation.get();
            let since = last.filter(|(g, _)| *g == generation).map_or(0, |(_, s)| s);
            if incoming.generation == generation
                && incoming.serial > since
                && !model.editing_text.get()
                && !model.dragging.get()
            {
                let samples = std::array::from_fn(|i| {
                    incoming.slots[i]
                        .filter(|(rev, rgb)| *rev > since && *rgb != slots[i])
                        .map(|(_, rgb)| rgb.map(|v| f64::from(v) / 255.0))
                });
                if samples.iter().any(Option::is_some) || active != incoming.active {
                    model.receive_slots(samples, incoming.active as u8, false);
                }
                return (generation, incoming.serial);
            }
            (generation, since)
        });
        on_cleanup_local(move || drop(effect));
        link
    }
    pub fn export_palette(self) {
        let mapping = self.model.mapping.get_untracked();
        let palette = if self.model.tab.get_untracked() == crate::workspace::Tab::Harmonies {
            PaletteExport {
                name: "OkColorPicker".into(),
                colors: self.model.palette.with_untracked(|items| {
                    items.iter().map(|(c, _)| rgb_bytes(*c, mapping)).collect()
                }),
            }
        } else {
            let folder = self.model.swatch_folder.get_untracked();
            self.model.library.with_untracked(|library| PaletteExport {
                name: library
                    .folders
                    .iter()
                    .find(|f| Some(f.id) == folder)
                    .map_or("OkColorPicker", |f| f.name.as_str())
                    .to_owned(),
                colors: library
                    .swatches_in(folder)
                    .map(|s| rgb_bytes(s.color(), mapping))
                    .collect(),
            })
        };
        if palette.colors.is_empty() {
            return;
        }
        self.channel.with_value(|tx| {
            if let Some(tx) = tx {
                let _ = tx.send(Command::Palette(self.generation.get_untracked(), palette));
            }
        });
    }
    pub fn state(self) -> State {
        self.inbox.with(|i| i.state.clone())
    }
    pub fn set(self, app: Option<PaintApp>) {
        let generation = self.generation.get_untracked() + 1;
        if let Some(app) = app {
            self.colors
                .with_value(|c| *c.lock().expect("paint colors") = Colors::read(self.model, app));
        }
        self.generation.set(generation);
        self.inbox.set(Incoming::new(
            generation,
            if app.is_some() {
                State::Connecting
            } else {
                State::Idle
            },
        ));
        if app.is_some() && self.channel.with_value(Option::is_none) {
            let (tx, rx) = mpsc::channel();
            let colors = self.colors.get_value();
            if let Err(e) = thread::Builder::new()
                .name("paint-link".into())
                .spawn(move || run(rx, colors, self.inbox))
            {
                self.inbox
                    .update(|i| i.state = State::Failed(e.to_string()));
                return;
            }
            self.channel.set_value(Some(tx));
        }
        self.channel.with_value(|tx| {
            if let Some(tx) = tx {
                let _ = tx.send(Command::Select(generation, app));
            }
        });
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct Swatch {
    r: u8,
    g: u8,
    b: u8,
    #[serde(default)]
    slot: usize,
}

fn config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(65536)
        .max_message_size(Some(16384))
        .max_frame_size(Some(16384))
}
type Socket = WebSocket<TcpStream>;
fn waiting(e: &tungstenite::Error) -> bool {
    matches!(e,tungstenite::Error::Io(e) if e.kind()==io::ErrorKind::WouldBlock)
}
#[allow(
    clippy::result_large_err,
    reason = "tungstenite handshake callback signature"
)]
fn check_origin(
    obsidian: bool,
    req: &Request,
    response: Response,
) -> Result<Response, ErrorResponse> {
    let allowed = req.headers().get("origin").is_none_or(|o| {
        if obsidian && o == "app://obsidian.md" {
            return true;
        }
        o.to_str()
            .ok()
            .and_then(|s| s.parse::<tungstenite::http::Uri>().ok())
            .and_then(|u| u.host().map(str::to_owned))
            .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]"))
    });
    if allowed {
        Ok(response)
    } else {
        Err(tungstenite::http::Response::builder()
            .status(403)
            .body(Some("Local clients only".into()))
            .expect("constant response"))
    }
}

struct Peer {
    socket: Socket,
    local: Option<Colors>,
    sent: [Option<Rgb>; 2],
    active: Option<usize>,
}
impl Peer {
    fn new(socket: Socket) -> io::Result<Self> {
        socket.get_ref().set_nonblocking(true)?;
        Ok(Self {
            socket,
            local: None,
            sent: [None; 2],
            active: None,
        })
    }
    fn tick(&mut self, app: PaintApp, colors: Colors, mut receive: impl FnMut(Swatch)) -> bool {
        // Only a new picker snapshot can originate an update. A remote edit can
        // arrive several frames before the UI thread publishes its new snapshot.
        let changed = self.local != Some(colors);
        self.local = Some(colors);
        if changed && !self.publish(app, colors) {
            return false;
        }
        for _ in 0..16 {
            match self.socket.read() {
                Ok(Message::Text(s)) if app == PaintApp::Obsidian => {
                    #[derive(Deserialize)]
                    struct Hex {
                        hex: String,
                    }
                    if let Ok(value) = serde_json::from_str::<Hex>(&s)
                        && let Some(hex) = value.hex.strip_prefix('#')
                        && hex.len() == 6
                        && hex.bytes().all(|b| b.is_ascii_hexdigit())
                        && let Ok(n) = u32::from_str_radix(hex, 16)
                    {
                        let [_, r, g, b] = n.to_be_bytes();
                        let slot = colors.active;
                        self.active = Some(slot);
                        self.sent[slot] = Some([r, g, b]);
                        receive(Swatch { slot, r, g, b });
                    }
                }
                Ok(Message::Text(s)) if matches!(app, PaintApp::Aseprite | PaintApp::Krita) => {
                    if let Ok(c) = serde_json::from_str::<Swatch>(&s)
                        && c.slot < 2
                    {
                        self.sent[c.slot] = Some([c.r, c.g, c.b]);
                        self.active = Some(c.slot);
                        receive(c);
                    }
                }
                Ok(Message::Close(_)) => return false,
                Ok(_) => {}
                Err(e) if waiting(&e) => break,
                Err(_) => return false,
            }
        }
        !self.socket.flush().is_err_and(|e| !waiting(&e))
    }
    fn publish(&mut self, app: PaintApp, colors: Colors) -> bool {
        for slot in [1 - colors.active, colors.active] {
            let rgb = colors.slots[slot];
            if matches!(app, PaintApp::Rebelle | PaintApp::Obsidian) && slot != colors.active {
                continue;
            }
            if self.sent[slot] == Some(rgb) && (slot != colors.active || self.active == Some(slot))
            {
                continue;
            }
            let [r, g, b] = rgb;
            let text = if app == PaintApp::Obsidian {
                serde_json::json!({"hex":format!("#{r:02X}{g:02X}{b:02X}")}).to_string()
            } else if matches!(app, PaintApp::Aseprite | PaintApp::Krita) {
                serde_json::to_string(&Swatch { r, g, b, slot }).expect("RGB packet")
            } else {
                serde_json::json!({"event_type":"SET_BRUSH_COLOR","color":{"r":r,"g":g,"b":b}})
                    .to_string()
            };
            match self.socket.send(Message::text(text)) {
                Ok(()) => {}
                Err(e) if waiting(&e) => {}
                Err(_) => return false,
            }
            self.sent[slot] = Some(rgb);
            self.active = Some(slot);
        }
        true
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.socket.close(None);
        let _ = self.socket.flush();
    }
}

#[allow(clippy::result_large_err, reason = "tungstenite callback signature")]
fn native_origin(req: &Request, resp: Response) -> Result<Response, ErrorResponse> {
    check_origin(false, req, resp)
}
#[allow(clippy::result_large_err, reason = "tungstenite callback signature")]
fn obsidian_origin(req: &Request, resp: Response) -> Result<Response, ErrorResponse> {
    check_origin(true, req, resp)
}
type Server = tungstenite::handshake::server::ServerHandshake<
    TcpStream,
    fn(&Request, Response) -> Result<Response, ErrorResponse>,
>;
type Pending = tungstenite::handshake::MidHandshake<Server>;
fn finish_handshake(
    result: Result<Socket, tungstenite::HandshakeError<Server>>,
    peers: &mut Vec<Peer>,
    pending: &mut Vec<(Instant, Pending)>,
    deadline: Instant,
) {
    match result {
        Ok(socket) => {
            if let Ok(peer) = Peer::new(socket) {
                peers.push(peer);
            }
        }
        Err(tungstenite::HandshakeError::Interrupted(step)) => pending.push((deadline, step)),
        _ => {}
    }
}

const TICK: Duration = Duration::from_millis(25);
fn run(rx: mpsc::Receiver<Command>, colors: Arc<Mutex<Colors>>, inbox: RwSignal<Incoming>) {
    let mut next = rx.recv().ok();
    while let Some(command) = next {
        let Command::Select(generation, app) = command else {
            next = rx.recv().ok();
            continue;
        };
        let Some(app) = app else {
            next = rx.recv().ok();
            continue;
        };
        let status = |state: State| {
            inbox.try_update(|i| {
                if i.generation == generation && i.state != state {
                    i.state = state;
                }
            });
        };
        if app.adobe() {
            next = adobe::session(app, generation, &rx, &colors, status);
            continue;
        }
        let listener = if let Some(port) = app.listen_port() {
            match TcpListener::bind((Ipv4Addr::LOCALHOST, port)).and_then(|l| {
                l.set_nonblocking(true)?;
                Ok(l)
            }) {
                Ok(l) => Some(l),
                Err(e) => {
                    status(State::Failed(format!("{} port {port}: {e}", app.name())));
                    next = rx.recv().ok();
                    continue;
                }
            }
        } else {
            None
        };
        let mut peers = Vec::<Peer>::new();
        let mut pending = Vec::new();
        let mut client = None;
        let mut retry = Instant::now();
        let mut last_state = None;
        loop {
            if let Some(l) = &listener {
                if peers.len() + pending.len() < 8
                    && let Ok((stream, _)) = l.accept()
                    && stream.set_nonblocking(true).is_ok()
                {
                    let handshake = tungstenite::accept_hdr_with_config(
                        stream,
                        (if app == PaintApp::Obsidian {
                            obsidian_origin
                        } else {
                            native_origin
                        })
                            as fn(&Request, Response) -> Result<Response, ErrorResponse>,
                        Some(config()),
                    );
                    finish_handshake(
                        handshake,
                        &mut peers,
                        &mut pending,
                        Instant::now() + Duration::from_secs(2),
                    );
                }
                for (deadline, step) in std::mem::take(&mut pending) {
                    if Instant::now() < deadline {
                        finish_handshake(step.handshake(), &mut peers, &mut pending, deadline);
                    }
                }
            } else {
                if peers.is_empty() && client.is_none() && Instant::now() >= retry {
                    retry = Instant::now() + Duration::from_secs(1);
                    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 8265));
                    if let Ok(stream) =
                        TcpStream::connect_timeout(&addr, Duration::from_millis(120))
                        && stream.set_nonblocking(true).is_ok()
                    {
                        client = Some((
                            Instant::now() + Duration::from_secs(2),
                            tungstenite::client::client_with_config(
                                "ws://127.0.0.1:8265/",
                                stream,
                                Some(config()),
                            ),
                        ));
                    }
                }
                if let Some((deadline, result)) = client.take() {
                    match result {
                        Ok((socket, _)) => {
                            if let Ok(peer) = Peer::new(socket) {
                                peers.push(peer);
                            }
                        }
                        Err(tungstenite::HandshakeError::Interrupted(step))
                            if Instant::now() < deadline =>
                        {
                            client = Some((deadline, step.handshake()))
                        }
                        _ => {}
                    }
                }
            }
            let current = *colors.lock().expect("paint colors");
            peers.retain_mut(|peer| {
                peer.tick(app, current, |swatch| {
                    inbox.try_update(|i| {
                        if i.generation == generation {
                            i.serial += 1;
                            i.active = swatch.slot;
                            i.slots[swatch.slot] = Some((i.serial, [swatch.r, swatch.g, swatch.b]));
                        }
                    });
                })
            });
            let state = if peers.is_empty() {
                State::Connecting
            } else {
                State::Connected
            };
            if last_state.as_ref() != Some(&state) {
                status(state.clone());
                last_state = Some(state);
            }
            match rx.recv_timeout(TICK) {
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Ok(Command::Palette(epoch, entries)) => {
                    if epoch == generation && app.palette_export() {
                        let text = serde_json::json!({"palette":entries.colors}).to_string();
                        peers.retain_mut(|p| {
                            !p.socket
                                .send(Message::text(text.clone()))
                                .is_err_and(|e| !waiting(&e))
                        });
                    }
                }
                Ok(command) => {
                    next = Some(command);
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tungstenite::protocol::Role;

    #[test]
    fn adobe_uses_committed_colors_while_other_hosts_receive_drag_previews() {
        Owner::new().with(|| {
            let model = PickerModel::default();
            let original = Colors::read(model, PaintApp::Photoshop).slots;
            let preview = Oklch::new(0.4, 0.0, 0.0, 1.0).unwrap();
            for commit in [false, true] {
                assert!(model.begin());
                model.preview(preview);
                let live = Colors::read(model, PaintApp::Krita).slots;
                assert_ne!(live, original);
                for app in [
                    PaintApp::Photoshop,
                    PaintApp::Illustrator,
                    PaintApp::InDesign,
                ] {
                    assert_eq!(Colors::read(model, app).slots, original);
                }
                if commit {
                    model.commit();
                } else {
                    model.cancel();
                }
                assert_eq!(
                    Colors::read(model, PaintApp::Photoshop).slots,
                    if commit { live } else { original }
                );
            }
        });
    }

    // Use connected loopback sockets and the real frame codecs in both directions.
    fn pair() -> (Peer, Socket) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let (server, _) = listener.accept().unwrap();
        (
            Peer::new(WebSocket::from_raw_socket(
                server,
                Role::Server,
                Some(config()),
            ))
            .unwrap(),
            WebSocket::from_raw_socket(stream, Role::Client, Some(config())),
        )
    }

    fn packet(peer: &mut Socket) -> Value {
        serde_json::from_str(peer.read().unwrap().to_text().unwrap()).unwrap()
    }

    #[test]
    fn aseprite_transfers_both_slots_and_never_echoes_a_stale_snapshot() {
        let (mut picker, mut app) = pair();
        let colors = Colors {
            slots: [[1, 2, 3], [4, 5, 6]],
            active: 0,
        };
        assert!(picker.tick(PaintApp::Aseprite, colors, |_| panic!()));
        assert_eq!(packet(&mut app), json!({"r":4,"g":5,"b":6,"slot":1}));
        assert_eq!(packet(&mut app), json!({"r":1,"g":2,"b":3,"slot":0}));
        app.send(Message::text(r#"{"r":21,"g":43,"b":65}"#))
            .unwrap();
        let mut received = None;
        for _ in 0..100 {
            assert!(picker.tick(PaintApp::Aseprite, colors, |c| received =
                Some((c.slot, [c.r, c.g, c.b]))));
            if received.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(received, Some((0, [21, 43, 65])));
        assert!(picker.tick(PaintApp::Aseprite, colors, |_| panic!()));
        let applied = Colors {
            slots: [[21, 43, 65], [4, 5, 6]],
            active: 0,
        };
        assert!(picker.tick(PaintApp::Aseprite, applied, |_| panic!()));
        // A real change to the background is followed by the active foreground.
        let edited = Colors {
            slots: [applied.slots[0], [7, 8, 9]],
            active: 0,
        };
        assert!(picker.tick(PaintApp::Aseprite, edited, |_| panic!()));
        assert_eq!(packet(&mut app), json!({"r":7,"g":8,"b":9,"slot":1}));
        assert_eq!(packet(&mut app), json!({"r":21,"g":43,"b":65,"slot":0}));
    }

    #[test]
    fn rebelle_sends_only_the_active_brush_and_replays_it_for_a_new_peer() {
        let colors = Colors {
            slots: [[8, 9, 10], [32, 48, 64]],
            active: 1,
        };
        for _ in 0..2 {
            let (mut picker, mut app) = pair();
            assert!(picker.tick(PaintApp::Rebelle, colors, |_| panic!()));
            assert_eq!(
                packet(&mut app),
                json!({"event_type":"SET_BRUSH_COLOR","color":{"r":32,"g":48,"b":64}})
            );
            assert!(picker.tick(
                PaintApp::Rebelle,
                Colors {
                    active: 0,
                    ..colors
                },
                |_| panic!()
            ));
            assert_eq!(
                packet(&mut app),
                json!({"event_type":"SET_BRUSH_COLOR","color":{"r":8,"g":9,"b":10}})
            );
        }
    }
}
