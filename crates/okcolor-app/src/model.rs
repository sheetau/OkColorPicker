use crate::persistence::{RECENT_LIMIT, SavedState};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, History, Oklch, format_color, parse_color};
use std::{
    hash::{BuildHasher, RandomState},
    sync::Arc,
};
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
    pub colors: [Oklch; 2],
    pub slot: u8,
    pub revision: u64,
    pub origin: EditOrigin,
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    library: Arc<crate::workspace::Library>,
    palette: Arc<Vec<(Oklch, bool)>>,
    colors: [Oklch; 2],
    contrast: std::sync::Arc<(crate::workspace::ContrastGrid, [f64; 4])>,
}

/// Each display dependency has its own signal; history is read only on commands.
#[derive(Clone, Copy)]
pub struct PickerModel {
    pub window_geometry: RwSignal<Option<crate::workspace::WindowGeometry>>,
    pub tab: RwSignal<crate::workspace::Tab>,
    pub palette: RwSignal<Vec<(Oklch, bool)>>,
    generator: StoredValue<okcolor_core::PaletteGenerator>,
    pub library: RwSignal<crate::workspace::Library>,
    pub tools: RwSignal<crate::workspace::ToolSettings>,
    pub companion: crate::companion::Companion,
    pub edit: RwSignal<ColorEdit>,
    pub dragging: RwSignal<bool>,
    pub display: RwSignal<crate::display::DisplayStatus>,
    pub l: RwSignal<f64>,
    pub c: RwSignal<f64>,
    pub h: RwSignal<f64>,
    pub alpha: RwSignal<f64>,
    pub gamut: RwSignal<Gamut>,
    pub show_p3: RwSignal<bool>,
    pub show_rec2020: RwSignal<bool>,
    pub mapping: RwSignal<GamutMapping>,
    pub format: RwSignal<ColorFormat>,
    pub max_chroma: RwSignal<f64>,
    pub recent: RwSignal<Vec<Oklch>>,
    pub slots: [RwSignal<Oklch>; 2],
    pub selected: RwSignal<u8>,
    pub can_undo: RwSignal<bool>,
    pub can_redo: RwSignal<bool>,
    pub message: RwSignal<String>,
    pub editing_text: RwSignal<bool>,
    pub storage_message: RwSignal<String>,
    pub can_save: RwSignal<bool>,
    pub save_request: RwSignal<u64>,
    committed: RwSignal<[Oklch; 2]>,
    history: RwSignal<History<Snapshot>, LocalStorage>,
    remote_edit: RwSignal<bool>,
}

impl Default for PickerModel {
    fn default() -> Self {
        Self::restore(SavedState::default())
    }
}

impl PickerModel {
    pub fn new(initial: Oklch) -> Self {
        Self::restore(SavedState {
            main: initial,
            ..SavedState::default()
        })
    }

    pub fn restore(saved: SavedState) -> Self {
        let colors = [saved.main, saved.sub];
        let initial = colors[usize::from(saved.selected)];
        let recent = History::restored(initial, 0, RECENT_LIMIT, saved.recent)
            .recent()
            .iter()
            .copied()
            .collect();
        let mut generator = okcolor_core::PaletteGenerator::default();
        let palette = generator.generate(&[], saved.gamut, RandomState::new().hash_one(()));
        let history = History::new(
            Snapshot {
                library: Arc::new(saved.library.clone()),
                palette: Arc::new(palette.clone()),
                colors,
                contrast: std::sync::Arc::new((
                    saved.tools.contrast_grid.clone(),
                    saved.tools.background,
                )),
            },
            100,
            0,
        );
        let [l, c, h, alpha] = initial.components();
        Self {
            tab: RwSignal::new(saved.tools.last_tab),
            window_geometry: RwSignal::new(saved.tools.window),
            library: RwSignal::new(saved.library),
            palette: RwSignal::new(palette),
            generator: StoredValue::new(generator),
            tools: RwSignal::new(saved.tools),
            companion: crate::companion::Companion::default(),
            edit: RwSignal::new(ColorEdit {
                color: initial,
                colors,
                slot: saved.selected,
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
            show_p3: RwSignal::new(saved.show_p3),
            show_rec2020: RwSignal::new(saved.show_rec2020),
            mapping: RwSignal::new(GamutMapping::LocalMinde),
            format: RwSignal::new(saved.format),
            max_chroma: RwSignal::new(saved.max_chroma),
            recent: RwSignal::new(recent),
            slots: colors.map(RwSignal::new),
            selected: RwSignal::new(saved.selected),
            can_undo: RwSignal::new(false),
            can_redo: RwSignal::new(false),
            message: RwSignal::new(String::new()),
            editing_text: RwSignal::new(false),
            storage_message: RwSignal::new(String::new()),
            can_save: RwSignal::new(false),
            save_request: RwSignal::new(0),
            committed: RwSignal::new(colors),
            history: RwSignal::new_local(history),
            remote_edit: RwSignal::new(false),
        }
    }

    /// Settings change the editing extent, never the original color or history.
    pub fn toggle_gamut(self, gamut: Gamut) {
        match gamut {
            Gamut::DisplayP3 => self.show_p3.update(|v| *v = !*v),
            Gamut::Rec2020 => self.show_rec2020.update(|v| *v = !*v),
            Gamut::Srgb => return,
        }
        let wide = self.show_rec2020.get_untracked();
        self.gamut.set(if wide {
            Gamut::Rec2020
        } else if self.show_p3.get_untracked() {
            Gamut::DisplayP3
        } else {
            Gamut::Srgb
        });
        self.max_chroma.set(if wide { 0.47 } else { 0.37 });
    }

    pub fn color(self) -> Oklch {
        Oklch::new(self.l.get(), self.c.get(), self.h.get(), self.alpha.get())
            .expect("validated model channels")
    }
    pub fn current(self) -> Oklch {
        self.colors()[usize::from(self.selected.get_untracked())]
    }

    pub fn colors(self) -> [Oklch; 2] {
        self.history.with_untracked(History::current).colors
    }

    fn replacing(self, color: Oklch) -> [Oklch; 2] {
        let mut colors = self.colors();
        colors[usize::from(self.selected.get_untracked())] = color;
        colors
    }

    /// Reactive durable state; preview channels deliberately are not dependencies.
    pub fn saved_state(self) -> SavedState {
        SavedState {
            main: self.committed.get()[0],
            sub: self.committed.get()[1],
            selected: self.selected.get(),
            recent: self.recent.get(),
            gamut: self.gamut.get(),
            show_p3: self.show_p3.get(),
            show_rec2020: self.show_rec2020.get(),
            mapping: self.mapping.get(),
            format: self.format.get(),
            max_chroma: self.max_chroma.get(),
            library: self.library.get(),
            tools: crate::workspace::ToolSettings {
                last_tab: self.tab.get(),
                window: self.window_geometry.get(),
                ..self.tools.get()
            },
        }
    }

    /// Shutdown also captures a commit made before the next reactive flush.
    pub fn saved_state_untracked(self) -> SavedState {
        let colors = self.history.with_untracked(History::committed).colors;
        SavedState {
            main: colors[0],
            sub: colors[1],
            selected: self.selected.get_untracked(),
            recent: self.recent.get_untracked(),
            gamut: self.gamut.get_untracked(),
            show_p3: self.show_p3.get_untracked(),
            show_rec2020: self.show_rec2020.get_untracked(),
            mapping: self.mapping.get_untracked(),
            format: self.format.get_untracked(),
            max_chroma: self.max_chroma.get_untracked(),
            library: self.library.get_untracked(),
            tools: crate::workspace::ToolSettings {
                last_tab: self.tab.get_untracked(),
                window: self.window_geometry.get_untracked(),
                ..self.tools.get_untracked()
            },
        }
    }

    fn record_color(self, color: Oklch) {
        self.recent.update(|recent| {
            recent.retain(|entry| *entry != color);
            recent.insert(0, color);
            recent.truncate(RECENT_LIMIT);
        });
    }
    fn record_recent(self) {
        self.record_color(self.current());
    }

    pub fn select_slot(self, slot: u8) {
        self.select_slot_from(slot, EditOrigin::Local);
    }
    pub(crate) fn select_slot_from(self, slot: u8, origin: EditOrigin) {
        if slot > 1 || self.dragging.get_untracked() || self.editing_text.get_untracked() {
            return;
        }
        if self.selected.get_untracked() != slot {
            self.finish_remote();
            self.selected.set(slot);
            self.publish_from(true, origin);
        }
    }
    pub fn swap(self) {
        if self.dragging.get_untracked() || self.editing_text.get_untracked() {
            return;
        }
        self.finish_remote();
        let [main, sub] = self.colors();
        let mut changed = false;
        self.history.update(|history| {
            changed = history.set(Snapshot {
                colors: [sub, main],
                ..history.current()
            }) == Ok(true)
        });
        if changed {
            self.record_recent();
        }
        self.publish(true);
    }

    pub fn select(self, color: Oklch) -> bool {
        self.select_from(color, EditOrigin::Local)
    }

    /// A remote commit uses the same Undo/recents contract without echoing back.
    pub fn receive_rgb(self, rgb: [f64; 3]) -> bool {
        self.receive_rgb_sample(rgb, false)
    }

    /// Remote gestures preview live, but commit one history entry on release.
    pub(crate) fn receive_rgb_sample(self, rgb: [f64; 3], manipulating: bool) -> bool {
        let mut samples = [None; 2];
        samples[usize::from(self.selected.get_untracked())] = Some(rgb);
        self.receive_slots(samples, self.selected.get_untracked(), manipulating)
    }

    pub(crate) fn receive_slots(
        self,
        samples: [Option<[f64; 3]>; 2],
        slot: u8,
        manipulating: bool,
    ) -> bool {
        if slot > 1 || self.editing_text.get_untracked() || self.dragging.get_untracked() {
            return false;
        }
        let mut colors = self.colors();
        for (color, rgb) in colors.iter_mut().zip(samples) {
            let Some(rgb) = rgb else { continue };
            if rgb
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return false;
            }
            let [_, _, h, alpha] = color.components();
            let Ok(next) = Oklch::from_rgb(rgb, Gamut::Srgb, alpha, h) else {
                return false;
            };
            *color = next;
        }
        if self.selected.get_untracked() != slot {
            self.finish_remote();
            self.selected.set(slot);
        }
        if !self.remote_edit.get_untracked() {
            let mut started = false;
            self.history
                .update(|history| started = history.begin().is_ok());
            if !started {
                return false;
            }
            self.remote_edit.set(true);
        }
        self.history.update(|history| {
            let _ = history.preview(Snapshot {
                colors,
                ..history.current()
            });
        });
        self.publish_from(false, EditOrigin::Companion);
        if !manipulating {
            self.finish_remote();
        }
        true
    }

    /// Preserve the last received sample if local editing or a disconnect ends a gesture.
    pub(crate) fn finish_remote(self) {
        if !self.remote_edit.get_untracked() {
            return;
        }
        let mut changed = false;
        self.history
            .update(|history| changed = history.commit() == Ok(true));
        if changed {
            self.record_recent();
        }
        self.remote_edit.set(false);
        self.publish_from(true, EditOrigin::Companion);
    }

    fn select_from(self, color: Oklch, origin: EditOrigin) -> bool {
        self.select_with_recent(color, origin, true)
    }

    fn select_with_recent(self, color: Oklch, origin: EditOrigin, add_recent: bool) -> bool {
        self.finish_remote();
        let colors = self.replacing(color);
        let mut accepted = true;
        self.history.update(|history| {
            let colors = Snapshot {
                colors,
                ..history.current()
            };
            match if add_recent {
                history.set(colors)
            } else {
                history.set_without_recent(colors)
            } {
                Ok(true) => {
                    if add_recent {
                        self.record_color(color);
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    accepted = false;
                    self.message.set(error.to_string());
                }
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
        self.set_channel_with_recent(channel, value, true)
    }

    pub(crate) fn set_channel_with_recent(
        self,
        channel: usize,
        value: f64,
        add_recent: bool,
    ) -> bool {
        let mut components = self.current().components();
        let Some(component) = components.get_mut(channel) else {
            return false;
        };
        *component = value;
        match Oklch::new(components[0], components[1], components[2], components[3]) {
            Ok(color) => {
                self.message.set(String::new());
                self.select_with_recent(color, EditOrigin::Local, add_recent)
            }
            Err(error) => {
                self.message.set(error.to_string());
                false
            }
        }
    }

    pub fn begin(self) -> bool {
        self.finish_remote();
        let mut started = false;
        self.history.update(|history| {
            started = history.begin().is_ok();
        });
        if started {
            self.publish(false);
        }
        started
    }
    pub fn preview(self, color: Oklch) {
        let colors = self.replacing(color);
        self.history.update(|history| {
            let _ = history.preview(Snapshot {
                colors,
                ..history.current()
            });
        });
        self.publish(false);
    }
    pub fn commit(self) {
        let mut changed = false;
        self.history
            .update(|history| changed = history.commit() == Ok(true));
        if changed {
            self.record_recent();
        }
        self.publish(true);
    }
    pub fn cancel(self) {
        self.history.update(|history| {
            let _ = history.cancel();
        });
        self.publish(false);
    }
    pub(crate) fn edit_contrast(self, edit: impl FnOnce(&mut crate::workspace::ToolSettings)) {
        self.finish_remote();
        let mut settings = self.tools.get_untracked();
        edit(&mut settings);
        let contrast = std::sync::Arc::new((settings.contrast_grid.clone(), settings.background));
        let mut changed = false;
        self.history.update(|history| {
            changed = history.set_without_recent(Snapshot {
                contrast,
                ..history.current()
            }) == Ok(true);
        });
        if changed {
            self.tools.set(settings);
            self.publish(true);
        }
    }

    pub(crate) fn edit_library<T>(
        self,
        edit: impl FnOnce(&mut crate::workspace::Library) -> Result<T, &'static str>,
    ) -> Option<T> {
        self.finish_remote();
        let mut library = self.library.get_untracked();
        let value = match edit(&mut library) {
            Ok(value) => value,
            Err(error) => {
                self.message.set(error.into());
                return None;
            }
        };
        let result = self
            .history
            .try_update(|history| {
                history.set_without_recent(Snapshot {
                    library: Arc::new(library.clone()),
                    ..history.current()
                })
            })?
            .ok()?;
        if result {
            self.library.set(library);
            self.publish(true);
        }
        self.message.set(String::new());
        Some(value)
    }

    pub(crate) fn edit_palette(self, edit: impl FnOnce(&mut Vec<(Oklch, bool)>)) {
        self.finish_remote();
        let mut palette = self.palette.get_untracked();
        edit(&mut palette);
        if self.history.try_update(|history| {
            history.set_without_recent(Snapshot {
                palette: Arc::new(palette.clone()),
                ..history.current()
            })
        }) == Some(Ok(true))
        {
            self.palette.set(palette);
            self.publish(true);
        }
    }

    pub(crate) fn preview_palette(self, palette: Vec<(Oklch, bool)>) {
        self.history.update(|history| {
            history
                .preview(Snapshot {
                    palette: Arc::new(palette.clone()),
                    ..history.current()
                })
                .expect("active palette adjustment");
        });
        self.palette.set(palette);
    }

    pub(crate) fn finish_palette_adjustment(self, commit: bool) {
        self.history.update(|history| {
            if commit {
                let _ = history.commit();
            } else {
                let _ = history.cancel();
            }
        });
        if !commit {
            self.restore_collections_and_contrast();
        }
        self.publish(commit);
    }

    pub(crate) fn generate_palette(self) {
        self.edit_palette(|palette| {
            self.generator.update_value(|generator| {
                *palette = generator.generate(
                    palette,
                    self.gamut.get_untracked(),
                    RandomState::new().hash_one(()),
                );
            });
        });
    }

    fn restore_collections_and_contrast(self) {
        let snapshot = self.history.with_untracked(History::current);
        if self.library.with_untracked(|v| {
            v.swatches != snapshot.library.swatches || v.folders != snapshot.library.folders
        }) {
            self.library
                .update(|v| v.restore_contents(&snapshot.library));
        }
        if self
            .palette
            .with_untracked(|v| v != snapshot.palette.as_ref())
        {
            self.palette.set((*snapshot.palette).clone());
        }
        if self.tools.with_untracked(|t| {
            t.contrast_grid != snapshot.contrast.0 || t.background != snapshot.contrast.1
        }) {
            self.tools.update(|t| {
                t.contrast_grid = snapshot.contrast.0.clone();
                t.background = snapshot.contrast.1;
            });
        }
    }

    pub fn undo(self) {
        self.finish_remote();
        self.history.update(|history| {
            let _ = history.undo();
        });
        self.restore_collections_and_contrast();
        self.publish(true);
    }
    pub fn redo(self) {
        self.finish_remote();
        self.history.update(|history| {
            let _ = history.redo();
        });
        self.restore_collections_and_contrast();
        self.publish(true);
    }

    /// Reference for derived palettes when the monitor uses the bounded preview path.
    pub fn palette_source(self) -> Oklch {
        Self::preview_source(self.color(), self.display.get())
    }

    pub fn palette_source_untracked(self) -> Oklch {
        Self::preview_source(self.current(), self.display.get_untracked())
    }

    pub(crate) fn preview_source(color: Oklch, display: crate::display::DisplayStatus) -> Oklch {
        if display.unavailable(color).is_some() {
            let [_, _, h, a] = color.components();
            if let Ok(mapped) = color
                .mapped_rgb(Gamut::Srgb, GamutMapping::LocalMinde)
                .and_then(|rgb| Oklch::from_rgb(rgb, Gamut::Srgb, a, h))
            {
                return mapped;
            }
        }
        color
    }

    pub fn output(self, format: ColorFormat) -> String {
        format_color(self.palette_source(), format, self.mapping.get())
            .map(|value| value.text)
            .unwrap_or_default()
    }

    /// Commands read a snapshot and must not create display subscriptions.
    pub fn output_untracked(self, format: ColorFormat) -> String {
        let color = self.palette_source_untracked();
        format_color(color, format, self.mapping.get_untracked())
            .map(|value| value.text)
            .unwrap_or_default()
    }

    fn publish(self, committed: bool) {
        self.publish_from(committed, EditOrigin::Local);
    }
    fn publish_from(self, committed: bool, origin: EditOrigin) {
        let color = self.current();
        let previous = self.edit.get_untracked();
        let colors = self.colors();
        let slot = self.selected.get_untracked();
        if previous.colors != colors || previous.slot != slot {
            self.edit.set(ColorEdit {
                color,
                colors,
                slot,
                origin,
                revision: previous
                    .revision
                    .checked_add(1)
                    .expect("color edit revision exhausted"),
            });
        }
        let dragging =
            self.history.with_untracked(History::is_editing) && !self.remote_edit.get_untracked();
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
        for (signal, value) in self.slots.into_iter().zip(colors) {
            if signal.get_untracked() != value {
                signal.set(value);
            }
        }
        self.history.with_untracked(|history| {
            self.can_undo.set(history.can_undo());
            self.can_redo.set(history.can_redo());
            if committed {
                let color = history.committed().colors;
                if self.committed.get_untracked() != color {
                    self.committed.set(color);
                }
            }
        });
    }
}
