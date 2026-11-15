//! Durable swatch library and tool preferences, independent of color edit history.
use okcolor_core::Oklch;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Picker,
    Swatches,
    Harmonies,
    Apca,
}
impl Tab {
    pub const ALL: [Self; 4] = [Self::Picker, Self::Swatches, Self::Harmonies, Self::Apca];
    pub fn next(self, backwards: bool) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap();
        Self::ALL[(index + if backwards { 3 } else { 1 }) % 4]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSettings {
    pub background: [f64; 4],
    pub target_lc: f64,
    pub harmony: usize,
}
impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            background: [1.0, 0.0, 0.0, 1.0],
            target_lc: 60.0,
            harmony: 0,
        }
    }
}
impl ToolSettings {
    pub fn validate(&self) -> Result<(), &'static str> {
        let [l, c, h, a] = self.background;
        if Oklch::new(l, c, h, a).is_err()
            || a != 1.0
            || !self.target_lc.is_finite()
            || !(-110.0..=110.0).contains(&self.target_lc)
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
