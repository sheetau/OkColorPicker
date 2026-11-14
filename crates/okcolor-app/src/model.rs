use crate::persistence::{RECENT_LIMIT, SavedState};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, History, Oklch, format_color, parse_color};
use zgui::prelude::*;

/// Shared stepping policy for channel fields, bars and the gamut slice.
pub(crate) fn stepped_channel(index: usize, base: f64, steps: f64, modifiers: Modifiers) -> f64 {
    let mut step = match index {
        1 => 0.001,
        2 => 1.0,
        _ => 0.01,
    };
    if modifiers.contains(Modifiers::SHIFT) {
        step *= 10.0;
    }
    if modifiers.contains(Modifiers::CONTROL) {
        step *= 0.1;
    }
    let value = base + steps * step;
    match index {
        0 | 3 => value.clamp(0.0, 1.0),
        1 => value.max(0.0),
        _ => value,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOrigin {
    Local,
    Companion,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorEdit {
    pub color: Oklch,
    pub revision: u64,
    pub origin: EditOrigin,
}

/// Each display dependency has its own signal; history is read only on commands.
#[derive(Clone, Copy)]
pub struct PickerModel {
    pub companion: crate::companion::Companion,
    pub edit: RwSignal<ColorEdit>,
    pub dragging: RwSignal<bool>,
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
            companion: crate::companion::Companion::default(),
            edit: RwSignal::new(ColorEdit {
                color: initial,
                revision: 0,
                origin: EditOrigin::Local,
            }),
            dragging: RwSignal::new(false),
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
        self.select_from(color, EditOrigin::Local)
    }

    /// A remote commit uses the same Undo/recents contract without echoing back.
    pub fn receive_rgb(self, rgb: [f64; 3]) -> bool {
        if self.editing_text.get_untracked()
            || self.dragging.get_untracked()
            || rgb
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return false;
        }
        let [_, _, h, alpha] = self.current().components();
        let Ok(color) = Oklch::from_rgb(rgb, Gamut::Srgb, alpha, h) else {
            return false;
        };
        self.select_from(color, EditOrigin::Companion)
    }

    fn select_from(self, color: Oklch, origin: EditOrigin) -> bool {
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
        self.publish_from(true, origin);
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
        self.publish_from(committed, EditOrigin::Local);
    }
    fn publish_from(self, committed: bool, origin: EditOrigin) {
        let color = self.current();
        let previous = self.edit.get_untracked();
        if previous.color != color {
            self.edit.set(ColorEdit {
                color,
                origin,
                revision: previous
                    .revision
                    .checked_add(1)
                    .expect("color edit revision exhausted"),
            });
        }
        let dragging = self.history.with_untracked(History::is_editing);
        if self.dragging.get_untracked() != dragging {
            self.dragging.set(dragging);
        }
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
