//! Durable swatch library and tool preferences.
use okcolor_core::Oklch;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaintApp {
    #[default]
    ClipStudio,
    Aseprite,
    Rebelle,
    Photoshop,
    Illustrator,
    InDesign,
    Krita,
    Obsidian,
}
impl PaintApp {
    pub const ALL: [Self; 8] = [
        Self::ClipStudio,
        Self::Photoshop,
        Self::Illustrator,
        Self::InDesign,
        Self::Aseprite,
        Self::Krita,
        Self::Rebelle,
        Self::Obsidian,
    ];
    pub fn adobe(self) -> bool {
        matches!(self, Self::Photoshop | Self::Illustrator | Self::InDesign)
    }
    pub fn palette_export(self) -> bool {
        self.adobe() || matches!(self, Self::Aseprite | Self::Krita)
    }
    pub fn listen_port(self) -> Option<u16> {
        match self {
            Self::Aseprite => Some(9009),
            Self::Obsidian => Some(9010),
            Self::Krita => Some(9011),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::ClipStudio => "CLIP STUDIO PAINT",
            Self::Photoshop => "Adobe Photoshop",
            Self::Illustrator => "Adobe Illustrator",
            Self::InDesign => "Adobe InDesign",
            Self::Krita => "Krita",
            Self::Obsidian => "Obsidian",
            Self::Aseprite => "Aseprite",
            Self::Rebelle => "Rebelle 8 Pro",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::ClipStudio => "CSP",
            Self::Photoshop => "PS",
            Self::Illustrator => "Ai",
            Self::InDesign => "Id",
            Self::Krita => "Krita",
            Self::Obsidian => "Ob",
            Self::Aseprite => "Ase",
            Self::Rebelle => "Reb",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tab {
    #[default]
    Picker,
    Harmonies,
    Apca,
}
impl Tab {
    pub const ALL: [Self; 3] = [Self::Picker, Self::Harmonies, Self::Apca];
    pub fn next(self, backwards: bool) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap();
        Self::ALL[(index + if backwards { 2 } else { 1 }) % 3]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Swatch {
    pub id: u64,
    pub name: String,
    pub color: [f64; 4],
    pub folder: Option<u64>,
}
impl Swatch {
    pub fn color(&self) -> Oklch {
        let [l, c, h, a] = self.color;
        Oklch::new(l, c, h, a).expect("validated library color")
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub id: u64,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    next_id: u64,
    pub folders: Vec<Folder>,
    pub swatches: Vec<Swatch>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            next_id: 1,
            folders: Vec::new(),
            swatches: Vec::new(),
        }
    }
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 64 && !name.chars().any(char::is_control)
}
impl Library {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.swatches.len() > 256 || self.folders.len() > 32 {
            return Err("Library capacity exceeded");
        }
        let mut ids = HashSet::new();
        for (id, name) in self
            .folders
            .iter()
            .map(|f| (f.id, f.name.as_str()))
            .chain(self.swatches.iter().map(|s| (s.id, s.name.as_str())))
        {
            if id == 0 || id >= self.next_id || !ids.insert(id) || !valid_name(name) {
                return Err("Invalid library identifier or name");
            }
        }
        if self.next_id == 0 {
            return Err("Invalid library sequence");
        }
        for swatch in &self.swatches {
            let [l, c, h, a] = swatch.color;
            if Oklch::new(l, c, h, a).is_err() || !self.has_folder(swatch.folder) {
                return Err("Invalid swatch color or folder");
            }
        }
        Ok(())
    }
    /// Undo restores the collection contents while keeping allocated IDs reserved.
    pub(crate) fn restore_contents(&mut self, snapshot: &Self) {
        self.folders.clone_from(&snapshot.folders);
        self.swatches.clone_from(&snapshot.swatches);
        self.next_id = self.next_id.max(snapshot.next_id);
    }

    fn has_folder(&self, folder: Option<u64>) -> bool {
        folder.is_none_or(|id| self.folders.iter().any(|f| f.id == id))
    }
    fn allocate(&mut self) -> Result<u64, &'static str> {
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or("Library identifier limit reached")?;
        Ok(id)
    }
    pub fn add(&mut self, color: Oklch, folder: Option<u64>) -> Result<u64, &'static str> {
        if self.swatches.len() >= 256 {
            return Err("Maximum 256 swatches");
        }
        if !self.has_folder(folder) {
            return Err("Folder no longer exists");
        }
        let id = self.allocate()?;
        self.swatches.push(Swatch {
            id,
            name: format!("Color {id}"),
            color: color.components(),
            folder,
        });
        Ok(id)
    }
    pub fn create_folder(&mut self, name: &str) -> Result<u64, &'static str> {
        if self.folders.len() >= 32 {
            return Err("Maximum 32 folders");
        }
        if !valid_name(name) {
            return Err("Enter a name of 1–64 characters");
        }
        let id = self.allocate()?;
        self.folders.push(Folder {
            id,
            name: name.trim().into(),
        });
        Ok(id)
    }
    pub fn rename(&mut self, id: u64, name: &str) -> Result<(), &'static str> {
        if !valid_name(name) {
            return Err("Enter a name of 1–64 characters");
        }
        if let Some(s) = self.swatches.iter_mut().find(|s| s.id == id) {
            s.name = name.trim().into();
        } else if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.name = name.trim().into();
        } else {
            return Err("Item no longer exists");
        }
        Ok(())
    }
    pub fn move_swatch(&mut self, id: u64, folder: Option<u64>) -> Result<(), &'static str> {
        if !self.has_folder(folder) {
            return Err("Folder no longer exists");
        }
        self.swatches
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("Swatch no longer exists")?
            .folder = folder;
        Ok(())
    }
    /// Removing a folder keeps its colors in the unfiled section.
    pub fn remove(&mut self, id: u64) {
        self.swatches.retain(|s| s.id != id);
        self.folders.retain(|f| f.id != id);
        for s in &mut self.swatches {
            if s.folder == Some(id) {
                s.folder = None;
            }
        }
    }
}

/// The component held constant across the two-dimensional chart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixedAxis {
    #[default]
    Hue,
    Lightness,
    Chroma,
}

impl FixedAxis {
    /// The menu names the plane by its plotted color distribution.
    pub(crate) fn view_name(self) -> &'static str {
        match self {
            Self::Hue => "Lightness",
            Self::Lightness => "Chroma",
            Self::Chroma => "Hue",
        }
    }
}

pub(crate) fn default_plot_height() -> f32 {
    152.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSettings {
    #[serde(default)]
    pub paint_app: PaintApp,
    #[serde(default)]
    pub last_tab: Tab,
    #[serde(default)]
    pub window: Option<WindowGeometry>,
    #[serde(default)]
    pub folded_groups: [bool; 3],
    #[serde(default)]
    pub contrast_grid: ContrastGrid,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default)]
    pub show_alpha_slider: bool,
    #[serde(default)]
    pub always_snap: bool,
    #[serde(default)]
    pub fixed_axis: FixedAxis,
    #[serde(default = "default_plot_height")]
    pub plot_height: f32,
    pub background: [f64; 4],
    pub target_lc: f64,
    pub harmony: usize,
}
/// Screen coordinates are physical. Client dimensions are logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            paint_app: PaintApp::default(),
            last_tab: Tab::Picker,
            window: None,
            folded_groups: [false; 3],
            always_on_top: false,
            show_alpha_slider: false,
            always_snap: false,
            fixed_axis: FixedAxis::default(),
            plot_height: default_plot_height(),
            contrast_grid: ContrastGrid::default(),
            background: [1.0, 0.0, 0.0, 1.0],
            target_lc: 60.0,
            harmony: 0,
        }
    }
}
impl ToolSettings {
    pub fn window_level(&self) -> zgui::platform::WindowLevel {
        if self.always_on_top {
            zgui::platform::WindowLevel::AlwaysOnTop
        } else {
            zgui::platform::WindowLevel::Normal
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.window.is_some_and(|g| {
            !g.width.is_finite()
                || !g.height.is_finite()
                || g.width <= 0.0
                || g.height <= 0.0
                || g.width > 65535.0
                || g.height > 65535.0
        }) {
            return Err("Invalid saved window dimensions");
        }
        self.contrast_grid.validate()?;
        let [l, c, h, a] = self.background;
        if Oklch::new(l, c, h, a).is_err()
            || a != 1.0
            || !self.target_lc.is_finite()
            || !(-110.0..=110.0).contains(&self.target_lc)
            || !self.plot_height.is_finite()
            || self.plot_height < 22.0
            || self.harmony >= 5
        {
            return Err("Invalid tool settings");
        }
        Ok(())
    }
    pub fn background(&self) -> Oklch {
        let [l, c, h, a] = self.background;
        Oklch::new(l, c, h, a).expect("validated opaque background")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContrastGrid {
    pub dark_reference: [f64; 4],
    pub levels: Vec<u8>,
    /// Optional explicit APCA polarities, in row order. Bare edits use the reference again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub positive_polarities: Vec<Option<bool>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub level_names: Vec<u32>,
    pub split: usize,
    pub to_text: bool,
    pub p3: bool,
    pub max_chroma: bool,
}
impl Default for ContrastGrid {
    fn default() -> Self {
        Self {
            dark_reference: [0.0, 0.0, 0.0, 1.0],
            levels: vec![70, 50, 50, 70],
            positive_polarities: Vec::new(),
            level_names: Vec::new(),
            split: 2,
            to_text: true,
            p3: true,
            max_chroma: true,
        }
    }
}
impl ContrastGrid {
    pub fn level_name(&self, index: usize) -> u32 {
        let step = if self.levels.len() <= 9 { 100 } else { 50 };
        (index as u32 + 1) * step
    }

    pub fn signed_target(&self, index: usize, reference: Oklch) -> f64 {
        let magnitude = f64::from(self.levels[index]);
        match self.positive_polarities.get(index).copied().flatten() {
            Some(true) => magnitude,
            Some(false) => -magnitude,
            None => okcolor_core::default_grid_target(reference, self.to_text, magnitude),
        }
    }

    pub fn set_target(&mut self, index: usize, score: f64, positive: Option<bool>) {
        self.levels[index] = score.abs().round() as u8;
        if positive.is_some() || !self.positive_polarities.is_empty() {
            self.positive_polarities.resize(self.levels.len(), None);
            self.positive_polarities[index] = positive;
            self.trim_polarities();
        }
    }

    fn trim_polarities(&mut self) {
        if self.positive_polarities.iter().all(Option::is_none) {
            self.positive_polarities.clear();
        }
    }

    pub fn insert(&mut self, index: usize) {
        if self.levels.len() >= 16 {
            return;
        }
        self.level_names.clear();
        let i = index.min(self.levels.len());
        let before = i
            .checked_sub(1)
            .and_then(|v| self.levels.get(v))
            .copied()
            .map(i16::from);
        let after = self.levels.get(i).copied().map(i16::from);
        let target = match (before, after) {
            (Some(a), Some(b)) => (a + b + 1) / 2,
            (Some(a), None) => {
                2 * a
                    - i.checked_sub(2)
                        .and_then(|v| self.levels.get(v))
                        .map_or(a - 5, |v| i16::from(*v))
            }
            (None, Some(b)) => 2 * b - self.levels.get(1).map_or(b + 5, |v| i16::from(*v)),
            _ => 60,
        };
        if !self.positive_polarities.is_empty() {
            self.positive_polarities.insert(i, None);
        }
        self.levels.insert(i, target.clamp(0, 110) as u8);
        if i < self.split || (i == self.split && self.split == self.levels.len() - 1) {
            self.split += 1;
        }
    }

    pub fn remove(&mut self, index: usize) {
        if self.levels.len() <= 1 || index >= self.levels.len() {
            return;
        }
        self.levels.remove(index);
        if !self.positive_polarities.is_empty() {
            self.positive_polarities.remove(index);
            self.trim_polarities();
        }
        self.level_names.clear();
        self.split -= usize::from(index < self.split);
    }

    fn validate(&self) -> Result<(), &'static str> {
        let [l, c, h, a] = self.dark_reference;
        if self.levels.is_empty()
            || (!self.positive_polarities.is_empty()
                && self.positive_polarities.len() != self.levels.len())
            || (!self.level_names.is_empty()
                && (self.level_names.len() != self.levels.len()
                    || self.level_names.windows(2).any(|p| p[0] >= p[1])))
            || self.levels.len() > 16
            || self.split > self.levels.len()
            || self.levels.iter().any(|lc| *lc > 110)
            || a != 1.0
            || Oklch::new(l, c, h, a).is_err()
        {
            return Err("Invalid contrast grid");
        }
        Ok(())
    }
}
