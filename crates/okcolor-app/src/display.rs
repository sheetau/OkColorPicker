//! Runtime preview policy, independent of source colors and saved preferences.
use std::{cell::Cell, rc::Rc};
use zgui::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode {
    #[default]
    Unknown,
    Sdr,
    WideColor,
    Hdr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayStatus {
    pub mode: DisplayMode,
    pub wide_output: bool,
}

impl DisplayStatus {
    pub fn unavailable(self, color: okcolor_core::Oklch) -> Option<&'static str> {
        use okcolor_core::Gamut;
        if !color.in_gamut(Gamut::Rec2020) && !color.in_gamut(Gamut::DisplayP3) {
            Some("unavailable on any device")
        } else if self.wide_output || color.in_gamut(Gamut::Srgb) {
            None
        } else if color.in_gamut(Gamut::DisplayP3) {
            Some("P3 is unavailable on this monitor")
        } else {
            Some("Rec.2020 is unavailable on this monitor")
        }
    }

    pub fn label(self) -> &'static str {
        match (self.mode, self.wide_output) {
            (DisplayMode::WideColor, true) => "Wide gamut · WCG",
            (DisplayMode::Hdr, true) => "Wide gamut · HDR",
            (DisplayMode::Sdr, _) => "sRGB preview · SDR",
            (DisplayMode::Unknown, _) => "sRGB preview · Display unknown",
            _ => "sRGB preview · SDR fallback",
        }
    }
}

/// One bridge per native window. Signals are bound only while that view exists.
#[derive(Clone, Default)]
pub struct DisplayBridge {
    status: Rc<Cell<DisplayStatus>>,
    signal: Rc<Cell<Option<RwSignal<DisplayStatus>>>>,
}

impl DisplayBridge {
    pub fn bind(&self, signal: RwSignal<DisplayStatus>) {
        assert!(
            self.signal.get().is_none(),
            "Use one display bridge per window"
        );
        signal.set(self.status.get());
        self.signal.set(Some(signal));
        let binding = self.signal.clone();
        on_cleanup_local(move || binding.set(None));
    }

    /// True means a mounted view must repaint before the new path is presented.
    pub fn publish(&self, status: DisplayStatus) -> bool {
        if self.status.replace(status) == status {
            return false;
        }
        if let Some(signal) = self.signal.get() {
            signal.set(status);
            true
        } else {
            false
        }
    }
}
