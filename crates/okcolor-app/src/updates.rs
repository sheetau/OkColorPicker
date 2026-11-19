//! The portable Windows package stages an update before offering either action.
use crate::model::PickerModel;
use zgui::prelude::*;

#[derive(Clone, Copy)]
pub(crate) struct Updates {
    pub visible: RwSignal<bool>,
    #[cfg(windows)]
    session: StoredValue<Option<windows::Session>>,
}

impl Updates {
    pub fn mount(model: PickerModel) -> Self {
        let state = Self {
            visible: RwSignal::new(false),
            #[cfg(windows)]
            session: StoredValue::new(None),
        };
        #[cfg(windows)]
        if let Some(helper) = windows::packaged_helper() {
            let window = use_window();
            spawn_local(async move {
                match blocking(move || windows::Session::start(helper)).await {
                    Ok(windows::Check::Ready(session)) => {
                        state.session.set_value(Some(session));
                        state.visible.set(true);
                    }
                    Ok(windows::Check::Install) => window.close(),
                    Ok(windows::Check::Current) => {}
                    Err(e) => model.message.set(format!("Update check failed: {e}")),
                }
            });
        }
        #[cfg(not(windows))]
        let _ = model;
        state
    }

    fn choose(self, now: bool, model: PickerModel) {
        #[cfg(windows)]
        if let Some(session) = self.session.try_update_value(Option::take).flatten() {
            let window = use_window();
            spawn_local(async move {
                match blocking(move || session.choose(now)).await {
                    Ok(true) => window.close(),
                    Ok(false) => self.visible.set(false),
                    Err(e) => {
                        self.visible.set(false);
                        model.message.set(format!("Update failed: {e}"));
                    }
                }
            });
        }
        #[cfg(not(windows))]
        let _ = (now, model);
    }
}

#[component]
pub(crate) fn UpdatePrompt(state: Updates, model: PickerModel) -> impl IntoView {
    let prompt = NodeRef::new();
    let focus = zgui::reactive::RenderEffect::new(move |_| {
        prompt.get()?;
        prompt.focus();
        Some(())
    });
    on_cleanup_local(move || drop(focus));
    view! {column(class="update-prompt",node_ref=prompt,tabindex=Focus::Programmatic) {
        text(class="update-title") {"An update is ready to install"}
        column(class="update-actions") {
        control(a11y:label="Install now",tabindex=Focus::Sequential,on:click=move |_|state.choose(true,model)) {"Install now"}
        control(a11y:label="Install on next launch",tabindex=Focus::Sequential,on:click=move |_|state.choose(false,model)) {"Install on next launch"}
        }
    }}
}

#[cfg(windows)]
mod windows {
    use std::{
        io::{self, BufRead, BufReader, Write},
        os::windows::process::CommandExt,
        path::PathBuf,
        process::{Child, ChildStdout, Command, Stdio},
    };

    pub(super) fn packaged_helper() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        if !exe
            .file_name()?
            .to_str()?
            .eq_ignore_ascii_case("OkColorPicker.exe")
        {
            return None;
        }
        let root = exe.parent()?;
        let script = root.join("update.ps1");
        (script.is_file() && root.join("PACKAGE.json").is_file()).then_some(script)
    }

    pub(super) enum Check {
        Current,
        Ready(Session),
        Install,
    }

    pub(super) struct Session {
        child: Child,
        replies: BufReader<ChildStdout>,
    }
    impl Session {
        pub fn start(helper: PathBuf) -> io::Result<Check> {
            let system = std::env::var_os("SystemRoot")
                .ok_or_else(|| io::Error::other("Missing SystemRoot"))?;
            let mut child = Command::new(
                PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            )
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(helper)
            .arg("-ParentId")
            .arg(std::process::id().to_string())
            .arg("-CurrentVersion")
            .arg(env!("CARGO_PKG_VERSION"))
            .env_remove("PSModulePath")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
            let replies = BufReader::new(child.stdout.take().expect("piped output"));
            let mut session = Self { child, replies };
            match session.read()?.as_str() {
                "CURRENT" => Ok(Check::Current),
                "READY" => Ok(Check::Ready(session)),
                "CLOSE" => Ok(Check::Install),
                message => Err(io::Error::other(message.to_owned())),
            }
        }
        fn read(&mut self) -> io::Result<String> {
            let mut line = String::new();
            if self.replies.read_line(&mut line)? == 0 {
                return Err(io::Error::other("Update helper ended before replying"));
            }
            Ok(line.trim().to_owned())
        }
        pub fn choose(mut self, now: bool) -> io::Result<bool> {
            let mut input = self.child.stdin.take().expect("piped input");
            writeln!(input, "{}", if now { "NOW" } else { "LATER" })?;
            drop(input);
            match self.read()?.as_str() {
                "CLOSE" => Ok(true),
                "DEFERRED" => Ok(false),
                message => Err(io::Error::other(message.to_owned())),
            }
        }
    }
}
