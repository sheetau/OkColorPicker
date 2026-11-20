//! Drive the running Adobe application through its Windows scripting interface.
//! A busy document can block COM, so keep one request in flight and coalesce edits.
use super::{Colors, Command, PaintApp, Rgb, State, TICK};
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
    let mut palette: Option<Vec<Rgb>> = None;
    let mut waiting = false;
    let mut check = Instant::now();
    let mut connected = false;
    loop {
        match host.replies.try_recv() {
            Ok(reply) => {
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
        let current = *colors.lock().expect("paint colors");
        let changed = sent != Some(current);
        if !waiting && (Instant::now() >= check || (connected && (changed || palette.is_some()))) {
            let slots = std::array::from_fn::<_, 2, _>(|i| {
                (sent.is_none_or(|last| last.slots[i] != current.slots[i]))
                    .then_some(current.slots[i])
            });
            let job = serde_json::json!({"slots":slots,"palette":palette.take()});
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
