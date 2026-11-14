use crate::persistence::{RECENT_LIMIT, SavedState};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, History, Oklch, format_color, parse_color};
use zgui::prelude::*;

/// Each display dependency has its own signal; history is read only on commands.
#[derive(Clone, Copy)]
pub struct PickerModel {
    pub display: RwSignal<crate::display::DisplayStatus>,
    pub l: RwSignal<f64>,
    pub c: RwSignal<f64>,
    pub h: RwSignal<f64>,
    pub alpha: RwSignal<f64>,
    pub gamut: RwSignal<Gamut>,
    pub mapping: RwSignal<GamutMapping>,
    pub format: RwSignal<ColorFormat>,
    pub max_chroma: RwSignal<f64>,
    pub recent: RwSignal<Vec<Oklch>>,
    pub previous: RwSignal<Oklch>,
    pub can_undo: RwSignal<bool>,
    pub can_redo: RwSignal<bool>,
    pub message: RwSignal<String>,
    pub editing_text: RwSignal<bool>,
    pub storage_message: RwSignal<String>,
    pub can_save: RwSignal<bool>,
    pub save_request: RwSignal<u64>,
    committed: RwSignal<Oklch>,
    history: RwSignal<History, LocalStorage>,
    before_drag: RwSignal<Oklch>,
}

impl Default for PickerModel {
    fn default() -> Self {
        Self::restore(SavedState::default())
    }
}

impl PickerModel {
    pub fn new(initial: Oklch) -> Self {
        Self::restore(SavedState {
            current: initial,
            previous: initial,
            ..SavedState::default()
        })
    }

    pub fn restore(saved: SavedState) -> Self {
        let initial = saved.current;
        let history = History::restored(initial, 100, RECENT_LIMIT, saved.recent);
        let recent = history.recent().iter().copied().collect();
        let [l, c, h, alpha] = initial.components();
        Self {
            display: RwSignal::new(crate::display::DisplayStatus::default()),
            l: RwSignal::new(l),
            c: RwSignal::new(c),
            h: RwSignal::new(h),
            alpha: RwSignal::new(alpha),
            gamut: RwSignal::new(saved.gamut),
            mapping: RwSignal::new(saved.mapping),
            format: RwSignal::new(saved.format),
            max_chroma: RwSignal::new(saved.max_chroma),
            recent: RwSignal::new(recent),
            previous: RwSignal::new(saved.previous),
            can_undo: RwSignal::new(false),
            can_redo: RwSignal::new(false),
            message: RwSignal::new(String::new()),
            editing_text: RwSignal::new(false),
            storage_message: RwSignal::new(String::new()),
            can_save: RwSignal::new(false),
            save_request: RwSignal::new(0),
            committed: RwSignal::new(initial),
            history: RwSignal::new_local(history),
            before_drag: RwSignal::new(initial),
        }
    }

    pub fn color(self) -> Oklch {
        Oklch::new(self.l.get(), self.c.get(), self.h.get(), self.alpha.get())
            .expect("validated model channels")
    }
    pub fn current(self) -> Oklch {
        self.history.with_untracked(History::current)
    }

    /// Reactive durable state; preview channels deliberately are not dependencies.
    pub fn saved_state(self) -> SavedState {
        SavedState {
            current: self.committed.get(),
            previous: self.previous.get(),
            recent: self.recent.get(),
            gamut: self.gamut.get(),
            mapping: self.mapping.get(),
            format: self.format.get(),
            max_chroma: self.max_chroma.get(),
        }
    }

    /// Shutdown also captures a commit made before the next reactive flush.
    pub fn saved_state_untracked(self) -> SavedState {
        let (current, recent) = self.history.with_untracked(|history| {
            (
                history.committed(),
                history.recent().iter().copied().collect(),
            )
        });
        SavedState {
            current,
            recent,
            previous: self.previous.get_untracked(),
            gamut: self.gamut.get_untracked(),
            mapping: self.mapping.get_untracked(),
            format: self.format.get_untracked(),
            max_chroma: self.max_chroma.get_untracked(),
        }
    }

    pub fn select(self, color: Oklch) -> bool {
        let before = self.current();
        let mut accepted = true;
        self.history.update(|history| match history.set(color) {
            Ok(true) => self.previous.set(before),
            Ok(false) => {}
            Err(error) => {
                accepted = false;
                self.message.set(error.to_string());
            }
        });
        self.publish(true);
        accepted
    }

    pub fn input(self, text: &str) -> bool {
        match parse_color(text, self.h.get_untracked()) {
            Ok(color) => {
                self.message.set(String::new());
                self.select(color)
            }
            Err(error) => {
                self.message.set(error.to_string());
                false
            }
        }
    }

    pub fn set_channel(self, channel: usize, value: f64) -> bool {
        let mut components = self.current().components();
        let Some(component) = components.get_mut(channel) else {
            return false;
        };
        *component = value;
        match Oklch::new(components[0], components[1], components[2], components[3]) {
            Ok(color) => {
                self.message.set(String::new());
                self.select(color)
            }
            Err(error) => {
                self.message.set(error.to_string());
                false
            }
        }
    }

    pub fn begin(self) -> bool {
        let mut started = false;
        self.history.update(|history| {
            started = history.begin().is_ok();
        });
        if started {
            self.before_drag.set(self.current());
            self.publish(false);
        }
        started
    }
    pub fn preview(self, color: Oklch) {
        self.history.update(|history| {
            let _ = history.preview(color);
        });
        self.publish(false);
    }
    pub fn commit(self) {
        self.history.update(|history| {
            if history.commit() == Ok(true) {
                self.previous.set(self.before_drag.get_untracked());
            }
        });
        self.publish(true);
    }
    pub fn cancel(self) {
        self.history.update(|history| {
            let _ = history.cancel();
        });
        self.publish(false);
    }
    pub fn undo(self) {
        let before = self.current();
        self.history.update(|history| {
            if history.undo() == Ok(true) {
                self.previous.set(before);
            }
        });
        self.publish(true);
    }
    pub fn redo(self) {
        let before = self.current();
        self.history.update(|history| {
            if history.redo() == Ok(true) {
                self.previous.set(before);
            }
        });
        self.publish(true);
    }

    pub fn output(self, format: ColorFormat) -> String {
        format_color(self.color(), format, self.mapping.get())
            .map(|value| value.text)
            .unwrap_or_default()
    }

    /// Commands read a snapshot and must not create display subscriptions.
    pub fn output_untracked(self, format: ColorFormat) -> String {
        format_color(self.current(), format, self.mapping.get_untracked())
            .map(|value| value.text)
            .unwrap_or_default()
    }

    fn publish(self, committed: bool) {
        let color = self.current();
        for (signal, value) in [self.l, self.c, self.h, self.alpha]
            .into_iter()
            .zip(color.components())
        {
            if signal.get_untracked() != value {
                signal.set(value);
            }
        }
        self.history.with_untracked(|history| {
            self.can_undo.set(history.can_undo());
            self.can_redo.set(history.can_redo());
            if committed {
                let color = history.committed();
                if self.committed.get_untracked() != color {
                    self.committed.set(color);
                }
                let recent = history.recent().iter().copied().collect::<Vec<_>>();
                if self.recent.get_untracked() != recent {
                    self.recent.set(recent);
                }
            }
        });
    }
}
