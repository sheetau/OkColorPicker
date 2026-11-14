//! Versioned user state. No display-rounded values or transient undo steps enter this file.
use okcolor_core::{ColorFormat, Gamut, GamutMapping, Oklch, parse_color};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub const RECENT_LIMIT: usize = 24;
const VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 128 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct SavedState {
    pub current: Oklch,
    pub previous: Oklch,
    /// Newest first; duplicates are removed when the history is restored.
    pub recent: Vec<Oklch>,
    pub gamut: Gamut,
    pub mapping: GamutMapping,
    pub format: ColorFormat,
    pub max_chroma: f64,
}

impl Default for SavedState {
    fn default() -> Self {
        let color = parse_color("#4e83a3", 0.0).expect("built-in color");
        Self {
            current: color,
            previous: color,
            recent: Vec::new(),
            gamut: Gamut::Srgb,
            mapping: GamutMapping::LocalMinde,
            format: ColorFormat::Hex,
            max_chroma: 0.4,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireState {
    version: u32,
    current: [f64; 4],
    previous: [f64; 4],
    recent: Vec<[f64; 4]>,
    gamut: String,
    mapping: String,
    format: String,
    max_chroma: f64,
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
            current: self.current.components(),
            previous: self.previous.components(),
            recent: self.recent.iter().map(|color| color.components()).collect(),
            gamut: gamut_name(self.gamut).into(),
            mapping: match self.mapping {
                GamutMapping::Clip => "clip",
                GamutMapping::LocalMinde => "local-minde",
            }
            .into(),
            format: format_name(self.format).into(),
            max_chroma: self.max_chroma,
        };
        let mut bytes = serde_json::to_vec_pretty(&wire).map_err(invalid)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid("State file exceeds 128 KiB"));
        }
        #[derive(Deserialize)]
        struct Header {
            version: u32,
        }
        let header: Header = serde_json::from_slice(bytes).map_err(invalid)?;
        if header.version != VERSION {
            return Err(invalid(format!(
                "Unsupported state version {}",
                header.version
            )));
        }
        let wire: WireState = serde_json::from_slice(bytes).map_err(invalid)?;
        let state = Self {
            current: read_color(wire.current)?,
            previous: read_color(wire.previous)?,
            recent: wire
                .recent
                .into_iter()
                .map(read_color)
                .collect::<io::Result<_>>()?,
            gamut: parse_gamut(&wire.gamut)?,
            mapping: match wire.mapping.as_str() {
                "clip" => GamutMapping::Clip,
                "local-minde" => GamutMapping::LocalMinde,
                _ => return Err(invalid("Unknown gamut mapping")),
            },
            format: parse_format(&wire.format)?,
            max_chroma: wire.max_chroma,
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
