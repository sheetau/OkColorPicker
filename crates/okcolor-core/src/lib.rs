//! Device-independent color calculations. Conversion never clips the source color.
#![forbid(unsafe_code)]

mod color;
mod format;
mod history;
mod matrix;
mod parse;
mod slice;

pub use color::{ColorError, Gamut, GamutMapping, Oklch};
pub use format::{ColorFormat, FormattedColor, format_color};
pub use history::{EditError, History};
pub use parse::{ParseColorError, parse_color};
pub use slice::{GamutProjection, GamutSlice};

mod contrast;
mod palette;
pub use palette::PaletteGenerator;
mod related;
pub use contrast::{
    apca, apca_in, closest_current_contrast, closest_grid_sample, contrast_sample,
    default_grid_target, lightness_for_contrast,
};
mod named;
pub use named::{NamedColor, color_name, delta_e_2000, nearest_named};
pub use related::{
    Harmony, Ramp, Variation, adjust_palette_color, harmony, ramp, ramp_steps, similar_named,
    variations,
};
