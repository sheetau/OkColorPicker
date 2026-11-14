use okcolor_core::{ColorFormat, Gamut, GamutMapping, History, Oklch, format_color, parse_color};
use zgui::prelude::*;

/// Each display dependency has its own signal; history is read only on commands.
#[derive(Clone, Copy)]
pub struct PickerModel {
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
    history: RwSignal<History, LocalStorage>,
    before_drag: RwSignal<Oklch>,
}

impl Default for PickerModel {
    fn default() -> Self {
        Self::new(parse_color("#4e83a3", 0.0).expect("built-in color"))
    }
}

impl PickerModel {
    pub fn new(initial: Oklch) -> Self {
        let [l, c, h, alpha] = initial.components();
        Self {
            l: RwSignal::new(l),
            c: RwSignal::new(c),
            h: RwSignal::new(h),
            alpha: RwSignal::new(alpha),
            gamut: RwSignal::new(Gamut::Srgb),
            mapping: RwSignal::new(GamutMapping::LocalMinde),
            format: RwSignal::new(ColorFormat::Hex),
            max_chroma: RwSignal::new(0.4),
            recent: RwSignal::new(Vec::new()),
            previous: RwSignal::new(initial),
            can_undo: RwSignal::new(false),
            can_redo: RwSignal::new(false),
            message: RwSignal::new(String::new()),
            history: RwSignal::new_local(History::new(initial, 100, 24)),
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
                self.recent.set(history.recent().iter().copied().collect());
            }
        });
    }
}
