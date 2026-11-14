//! UI bridge for a runtime-only CSP connection. Credentials never enter saved state.
use okcolor_csp::worker::{Control, Event, State, Worker};
use zgui::{prelude::*, reactive::RenderEffect};

#[derive(Clone, Copy)]
pub struct Companion {
    pub state: RwSignal<State>,
    control: RwSignal<Option<Control>>,
}
impl Default for Companion {
    fn default() -> Self {
        Self {
            state: RwSignal::new(State::Idle),
            control: RwSignal::new(None),
        }
    }
}
impl Companion {
    pub fn bind(self) {
        let incoming = RwSignal::new(None::<(u64, State)>);
        let worker = Worker::start(
            |cancelled| {
                #[cfg(target_os = "windows")]
                {
                    crate::windows_qr::scan(cancelled)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = cancelled;
                    Err(okcolor_csp::qr::ScanError::CaptureUnavailable)
                }
            },
            move |generation, event| {
                if let Event::State(state) = event {
                    incoming.try_set(Some((generation, state)));
                }
            },
        );
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
        let effect = RenderEffect::new(move |_| {
            if let Some((generation, state)) = incoming.get()
                && generation == control.generation()
            {
                self.state.set(state);
            }
        });
        on_cleanup_local(move || {
            drop(effect);
            drop(worker);
        });
    }
    pub fn scan(self) {
        if let Some(control) = self.control.get_untracked() {
            control.scan();
            self.state.set(State::Scanning);
        }
    }
    pub fn disconnect(self) {
        if let Some(control) = self.control.get_untracked() {
            control.disconnect();
        }
        self.state.set(State::Idle);
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
