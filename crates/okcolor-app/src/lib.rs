//! Native application components, kept separate from the color calculation core.
pub mod autosave;
mod chart;
pub mod companion;
mod companion_sync;
pub mod display;
pub mod fonts;
pub mod graphics;
pub mod model;
pub mod output_pass;
pub mod persistence;
#[cfg(target_os = "windows")]
pub mod presentation;
pub mod surface;
pub mod ui;
#[cfg(target_os = "windows")]
mod unavailable_sink;
pub mod window_frame;
#[cfg(target_os = "windows")]
pub mod windows_caption;
#[cfg(target_os = "windows")]
mod windows_color_events;
#[cfg(target_os = "windows")]
mod windows_composition;
#[cfg(target_os = "windows")]
pub mod windows_dispatcher;
#[cfg(target_os = "windows")]
pub mod windows_display;
#[cfg(target_os = "windows")]
mod windows_display_watch;
#[cfg(target_os = "windows")]
mod windows_qr;

mod eyedropper;
#[cfg(windows)]
mod frozen_desktop;
#[cfg(windows)]
mod frozen_hdr;
mod menu;
mod swatches;
mod tiles;
mod tools_ui;
mod tooltip;
mod updates;
pub mod workspace;
