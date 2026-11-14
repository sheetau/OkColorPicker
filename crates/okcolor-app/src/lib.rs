//! Native application components, kept separate from the color calculation core.
pub mod autosave;
pub mod fonts;
pub mod graphics;
pub mod model;
pub mod persistence;
pub mod surface;
pub mod ui;
pub mod window_frame;
#[cfg(target_os = "windows")]
mod windows_composition;
