//! Versioned user state. No display-rounded values or transient undo steps enter this file.
use crate::workspace::{Library, ToolSettings};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, Oklch, parse_color};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub const RECENT_LIMIT: usize = 24;
const VERSION: u32 = 4;
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct SavedState {
    pub main: Oklch,
    pub sub: Oklch,
    pub selected: u8,
    /// Newest first; duplicates are removed when the history is restored.
    pub recent: Vec<Oklch>,
    pub gamut: Gamut,
    pub show_p3: bool,
    pub show_rec2020: bool,
    pub mapping: GamutMapping,
    pub format: ColorFormat,
    pub max_chroma: f64,
    pub library: Library,
    pub tools: ToolSettings,
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            main: parse_color("#ca006a", 0.0).expect("built-in color"),
            sub: parse_color("#000", 0.0).expect("built-in color"),
            selected: 0,
            recent: Vec::new(),
            gamut: Gamut::Srgb,
            show_p3: false,
            show_rec2020: false,
            mapping: GamutMapping::LocalMinde,
            format: ColorFormat::Hex,
            max_chroma: 0.37,
            library: Library::default(),
            tools: ToolSettings::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireState {
    version: u32,
    main: [f64; 4],
    sub: [f64; 4],
    selected: u8,
    recent: Vec<[f64; 4]>,
    gamut: String,
    show_p3: bool,
    show_rec2020: bool,
    mapping: String,
    format: String,
    max_chroma: f64,
    library: Library,
    tools: ToolSettings,
}

fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn gamut_name(gamut: Gamut) -> &'static str {
    match gamut {
        Gamut::Srgb => "srgb",
        Gamut::DisplayP3 => "display-p3",
        Gamut::Rec2020 => "rec2020",
    }
}
fn parse_gamut(name: &str) -> io::Result<Gamut> {
    match name {
        "srgb" => Ok(Gamut::Srgb),
        "display-p3" => Ok(Gamut::DisplayP3),
        "rec2020" => Ok(Gamut::Rec2020),
        _ => Err(invalid("Unknown working gamut")),
    }
}
fn format_name(format: ColorFormat) -> &'static str {
    match format {
        ColorFormat::Hex => "hex",
        ColorFormat::Hex8 => "hex8",
        ColorFormat::Rgb => "rgb",
        ColorFormat::Hsl => "hsl",
        ColorFormat::Oklab => "oklab",
        ColorFormat::Oklch => "oklch",
        ColorFormat::Lab => "lab",
        ColorFormat::Lch => "lch",
        ColorFormat::Color(gamut) => gamut_name(gamut),
    }
}
fn parse_format(name: &str) -> io::Result<ColorFormat> {
    Ok(match name {
        "hex" => ColorFormat::Hex,
        "hex8" => ColorFormat::Hex8,
        "rgb" => ColorFormat::Rgb,
        "hsl" => ColorFormat::Hsl,
        "oklab" => ColorFormat::Oklab,
        "oklch" => ColorFormat::Oklch,
        "lab" => ColorFormat::Lab,
        "lch" => ColorFormat::Lch,
        other => {
            ColorFormat::Color(parse_gamut(other).map_err(|_| invalid("Unknown output format"))?)
        }
    })
}
fn read_color(value: [f64; 4]) -> io::Result<Oklch> {
    Oklch::new(value[0], value[1], value[2], value[3]).map_err(invalid)
}

impl SavedState {
    fn validate(&self) -> io::Result<()> {
        self.library.validate().map_err(invalid)?;
        self.tools.validate().map_err(invalid)?;
        if self.selected > 1 {
            return Err(invalid("Unknown color slot"));
        }
        if self.recent.len() > RECENT_LIMIT {
            return Err(invalid("Too many recent colors"));
        }
        if !self.max_chroma.is_finite() || !(0.01..=1.0).contains(&self.max_chroma) {
            return Err(invalid("Chroma display range must be between 0.01 and 1"));
        }
        Ok(())
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        self.validate()?;
        let wire = WireState {
            version: VERSION,
            main: self.main.components(),
            sub: self.sub.components(),
            selected: self.selected,
            recent: self.recent.iter().map(|color| color.components()).collect(),
            gamut: gamut_name(self.gamut).into(),
            show_p3: self.show_p3,
            show_rec2020: self.show_rec2020,
            mapping: match self.mapping {
                GamutMapping::Clip => "clip",
                GamutMapping::LocalMinde => "local-minde",
            }
            .into(),
            format: format_name(self.format).into(),
            max_chroma: self.max_chroma,
            library: self.library.clone(),
            tools: self.tools.clone(),
        };
        let mut bytes = serde_json::to_vec_pretty(&wire).map_err(invalid)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid("State file exceeds 256 KiB"));
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid("State file exceeds 256 KiB"));
        }
        #[derive(Deserialize)]
        struct Header {
            version: u32,
        }
        let header: Header = serde_json::from_slice(bytes).map_err(invalid)?;
        if header.version == 1 {
            // Version 1 stored a current color and the previous edit, not two slots.
            // Preserve both originals as main/sub and start on main.
            let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(invalid)?;
            let object = value
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid state object"))?;
            if object.contains_key("main")
                || object.contains_key("sub")
                || object.contains_key("selected")
            {
                return Err(invalid("Unexpected version 1 slot fields"));
            }
            let current = object
                .remove("current")
                .ok_or_else(|| invalid("Missing current color"))?;
            let previous = object
                .remove("previous")
                .ok_or_else(|| invalid("Missing previous color"))?;
            object.insert("main".into(), current);
            object.insert("sub".into(), previous);
            object.insert("selected".into(), 0.into());
            object.insert("version".into(), 2.into());
            return Self::decode(&serde_json::to_vec(&value).map_err(invalid)?);
        }
        if header.version == 2 {
            let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(invalid)?;
            let object = value
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid state object"))?;
            if object.contains_key("show_p3") || object.contains_key("show_rec2020") {
                return Err(invalid("Unexpected version 2 gamut flags"));
            }
            let gamut = parse_gamut(
                object
                    .get("gamut")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| invalid("Missing working gamut"))?,
            )?;
            let old_extent = object
                .get("max_chroma")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| invalid("Missing chroma range"))?;
            if !old_extent.is_finite() || !(0.01..=1.0).contains(&old_extent) {
                return Err(invalid("Invalid legacy chroma range"));
            }
            object.insert("show_p3".into(), (gamut == Gamut::DisplayP3).into());
            object.insert("show_rec2020".into(), (gamut == Gamut::Rec2020).into());
            object.insert(
                "max_chroma".into(),
                if gamut == Gamut::Rec2020 {
                    0.47.into()
                } else {
                    0.37.into()
                },
            );
            object.insert("version".into(), 3.into());
            return Self::decode(&serde_json::to_vec(&value).map_err(invalid)?);
        }
        if header.version == 3 {
            let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(invalid)?;
            let object = value
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid state object"))?;
            if object.contains_key("library") || object.contains_key("tools") {
                return Err(invalid("Unexpected version 3 tool fields"));
            }
            object.insert(
                "library".into(),
                serde_json::to_value(Library::default()).map_err(invalid)?,
            );
            object.insert(
                "tools".into(),
                serde_json::to_value(ToolSettings::default()).map_err(invalid)?,
            );
            object.insert("version".into(), VERSION.into());
            return Self::decode(&serde_json::to_vec(&value).map_err(invalid)?);
        }
        if header.version != VERSION {
            return Err(invalid(format!(
                "Unsupported state version {}",
                header.version
            )));
        }
        let wire: WireState = serde_json::from_slice(bytes).map_err(invalid)?;
        let state = Self {
            main: read_color(wire.main)?,
            sub: read_color(wire.sub)?,
            selected: wire.selected,
            recent: wire
                .recent
                .into_iter()
                .map(read_color)
                .collect::<io::Result<_>>()?,
            gamut: parse_gamut(&wire.gamut)?,
            show_p3: wire.show_p3,
            show_rec2020: wire.show_rec2020,
            mapping: match wire.mapping.as_str() {
                "clip" => GamutMapping::Clip,
                "local-minde" => GamutMapping::LocalMinde,
                _ => return Err(invalid("Unknown gamut mapping")),
            },
            format: parse_format(&wire.format)?,
            max_chroma: wire.max_chroma,
            library: wire.library,
            tools: wire.tools,
        };
        state.validate()?;
        Ok(state)
    }
}

/// One writable profile. The lock remains held across atomic replacements of state.json.
pub struct StateStore {
    directory: PathBuf,
    _lock: File,
}

impl StateStore {
    pub fn open(directory: &Path) -> io::Result<Self> {
        if !directory.is_absolute() {
            return Err(invalid("State directory must be an absolute path"));
        }
        fs::create_dir_all(directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("state.lock"))?;
        lock.try_lock().map_err(|error| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("Cannot lock state profile: {error}"),
            )
        })?;
        Ok(Self {
            directory: directory.to_path_buf(),
            _lock: lock,
        })
    }

    pub fn load(&self) -> io::Result<SavedState> {
        let file = match File::open(self.directory.join("state.json")) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(SavedState::default());
            }
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        SavedState::decode(&bytes)
    }

    pub fn save(&self, state: &SavedState) -> io::Result<()> {
        // Serialize and validate before touching the filesystem. A failed write or replacement
        // leaves the previous destination in place; the temporary file cleans itself up.
        let bytes = state.encode()?;
        let mut staged = tempfile::Builder::new()
            .prefix(".state-")
            .suffix(".tmp")
            .tempfile_in(&self.directory)?;
        staged.write_all(&bytes)?;
        staged.as_file().sync_all()?;
        staged
            .persist(self.directory.join("state.json"))
            .map_err(|error| error.error)?;
        #[cfg(unix)]
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}

pub struct StartupState {
    pub state: SavedState,
    pub store: Option<StateStore>,
    pub warning: String,
}

impl StartupState {
    pub fn open(directory: io::Result<PathBuf>) -> Self {
        let loaded = directory.and_then(|directory| {
            let store = StateStore::open(&directory)?;
            let state = store.load()?;
            Ok((state, store))
        });
        match loaded {
            Ok((state, store)) => Self {
                state,
                store: Some(store),
                warning: String::new(),
            },
            Err(error) => {
                eprintln!(
                    "State restoration failed; existing data will not be overwritten: {error}"
                );
                Self {
                    state: SavedState::default(),
                    store: None,
                    warning: format!("Session only — saved state unavailable: {error}"),
                }
            }
        }
    }
}

/// An explicit override supports portable profiles and isolated tests; never fall back to cwd.
pub fn state_directory() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("OKCOLOR_STATE_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")));
    base.map(|path| path.join("OkColorPicker"))
        .ok_or_else(|| invalid("No user state directory is available"))
}
