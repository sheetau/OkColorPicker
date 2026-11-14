//! Device-independent color calculations. Conversion never clips the source color.
#![forbid(unsafe_code)]

mod color;
mod matrix;

pub use color::{ColorError, Gamut, Oklch};
