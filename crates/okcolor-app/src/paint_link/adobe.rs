//! Drive the running Adobe application through its Windows scripting interface.
//! A busy document can block COM, so keep one request in flight and coalesce edits.
use super::{Colors, Command, PaintApp, PaletteExport, State, TICK};
use std::{
    io::{self, BufRead, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

struct Host {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<String>,
}

impl Host {
    fn start(app: PaintApp) -> io::Result<Self> {
        let program = match app {
            PaintApp::Photoshop => "Photoshop",
            PaintApp::Illustrator => "Illustrator",
            PaintApp::InDesign => "InDesign",
            _ => return Err(io::Error::other("Expected an Adobe application")),
        };
        let system = std::env::var_os("SystemRoot")
            .ok_or_else(|| io::Error::other("Windows scripting is unavailable"))?;
        let mut cmd = std::process::Command::new(
            std::path::PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd
            .args([
                "-NoProfile",
                "-NoLogo",
                "-NonInteractive",
                "-STA",
                "-Command",
                include_str!("adobe.ps1"),
            ])
            .env("OKCOLOR_ADOBE_APP", program)
            .env("OKCOLOR_ADOBE_SCRIPT", include_str!("colors.jsx"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, replies) = mpsc::channel();
        let host = Self {
            input: child.stdin.take().expect("piped stdin"),
            child,
            replies,
        };
        thread::Builder::new()
            .name("adobe-replies".into())
            .spawn(move || {
                for line in io::BufReader::new(stdout).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })?;
        Ok(host)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// Adobe Swatch Exchange uses big-endian block lengths and NUL-terminated UTF-16 names.
fn swatch_file(
    directory: &std::path::Path,
    palette: &PaletteExport,
) -> io::Result<std::path::PathBuf> {
    fn name(value: &str) -> Vec<u8> {
        let units: Vec<_> = value.encode_utf16().chain([0]).collect();
        let mut bytes = (units.len() as u16).to_be_bytes().to_vec();
        bytes.extend(units.into_iter().flat_map(u16::to_be_bytes));
        bytes
    }
    fn block(file: &mut Vec<u8>, kind: u16, data: &[u8]) {
        file.extend(kind.to_be_bytes());
        file.extend((data.len() as u32).to_be_bytes());
        file.extend(data);
    }
    let mut file = b"ASEF\0\x01\0\0".to_vec();
    file.extend(((palette.colors.len() + 2) as u32).to_be_bytes());
    block(&mut file, 0xc001, &name(&palette.name));
    for &[r, g, b] in &palette.colors {
        let mut data = name(&format!("OkColorPicker #{r:02X}{g:02X}{b:02X}"));
        data.extend(b"RGB ");
        for channel in [r, g, b] {
            data.extend((f32::from(channel) / 255.0).to_be_bytes());
        }
        data.extend(2_u16.to_be_bytes()); // Normal process color
        block(&mut file, 1, &data);
    }
    block(&mut file, 0xc002, &[]);
    let stem: String = palette
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = directory.join(format!("Swatches - {stem}.ase"));
    std::fs::write(&path, file)?;
    Ok(path)
}

pub(super) fn session(
    app: PaintApp,
    generation: u64,
    rx: &mpsc::Receiver<Command>,
    colors: &Arc<Mutex<Colors>>,
    status: impl Fn(State),
) -> Option<Command> {
    let mut host = match Host::start(app) {
        Ok(child) => child,
        Err(e) => {
            status(State::Failed(format!("{}: {e}", app.name())));
            return rx.recv().ok();
        }
    };
    let mut sent: Option<Colors> = None;
    let mut observed = *colors.lock().expect("paint colors");
    let mut last_edit = Instant::now();
    let mut palette: Option<PaletteExport> = None;
    let mut exchange_directory = None;
    let mut waiting = false;
    let mut check = Instant::now();
    let mut connected = false;
    loop {
        match host.replies.try_recv() {
            Ok(reply) => {
                drop(exchange_directory.take());
                waiting = false;
                match reply.trim() {
                    "OK" => {
                        if !connected {
                            status(State::Connected);
                        }
                        connected = true;
                        check = Instant::now() + Duration::from_secs(2);
                    }
                    "WAIT" => {
                        if connected {
                            status(State::Connecting);
                        }
                        connected = false;
                        sent = None;
                        check = Instant::now() + Duration::from_millis(700);
                    }
                    _ => {
                        status(State::Failed(format!(
                            "{} could not apply the color. Check the active document.",
                            app.short()
                        )));
                        return rx.recv().ok();
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                status(State::Failed(format!(
                    "{} scripting session closed",
                    app.short()
                )));
                return rx.recv().ok();
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let latest = *colors.lock().expect("paint colors");
        if latest != observed {
            observed = latest;
            last_edit = Instant::now();
        }
        // Wheel detents arrive as separate committed edits without a reliable end event.
        let current = if last_edit.elapsed() < Duration::from_millis(180) {
            sent.unwrap_or(latest)
        } else {
            latest
        };
        let changed = sent != Some(current);
        if !waiting && (Instant::now() >= check || (connected && (changed || palette.is_some()))) {
            let slots = std::array::from_fn::<_, 2, _>(|i| {
                (sent.is_none_or(|last| {
                    last.slots[i] != current.slots[i] || last.none[i] != current.none[i]
                }))
                .then_some(current.slots[i])
            });
            let mut job = serde_json::json!({"slots": slots, "none": current.none});
            if app == PaintApp::InDesign && sent.is_none_or(|last| last.active != current.active) {
                job["active"] = serde_json::json!(current.active);
            }
            if let Some(entries) = palette.take() {
                if app == PaintApp::Illustrator {
                    job["palette"] = serde_json::json!(entries.colors);
                } else {
                    // Keep the file until the host acknowledges the request.
                    let file = tempfile::tempdir().and_then(|dir| {
                        let path = swatch_file(dir.path(), &entries)?;
                        exchange_directory = Some(dir);
                        Ok(path)
                    });
                    match file {
                        Ok(path) => job["swatchFile"] = serde_json::json!(path),
                        Err(e) => {
                            status(State::Failed(format!("Could not prepare swatches: {e}")));
                            return rx.recv().ok();
                        }
                    }
                }
            }
            if writeln!(host.input, "{job}")
                .and_then(|()| host.input.flush())
                .is_err()
            {
                status(State::Failed(format!(
                    "{} scripting connection lost",
                    app.short()
                )));
                return rx.recv().ok();
            }
            sent = Some(current);
            waiting = true;
        }
        match rx.recv_timeout(TICK) {
            Ok(Command::Palette(epoch, entries)) => {
                if epoch == generation && connected {
                    palette = Some(entries);
                }
            }
            Ok(select) => return Some(select),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}
