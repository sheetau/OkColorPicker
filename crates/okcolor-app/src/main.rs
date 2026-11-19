#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use okcolor_app::{
    autosave, fonts, graphics,
    model::PickerModel,
    persistence::{StartupState, state_directory},
    ui::{self, PickerProps, PickerRefs},
};
#[cfg(not(target_os = "windows"))]
use std::cell::Cell;
use std::rc::Rc;
use zgui::prelude::*;

#[derive(Default)]
#[cfg(not(target_os = "windows"))]
struct FrameCount(Cell<u64>);
#[cfg(not(target_os = "windows"))]
impl zgui::runtime::FrameProbe for FrameCount {
    fn frame_ended(&self, _window: &zgui::runtime::Window) {
        let frame = self.0.get() + 1;
        self.0.set(frame);
        eprintln!("Rendered frame {frame}");
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("OkColorPicker could not run: {error}");
        #[cfg(all(target_os = "windows", not(debug_assertions)))]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
            let message: Vec<u16> = format!("OkColorPicker could not run.\n\n{error}")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let title: Vec<u16> = "OkColorPicker".encode_utf16().chain(Some(0)).collect();
            // The GUI subsystem has no console for an Explorer-launched application.
            // Keep both UTF-16 buffers alive through this synchronous native dialog.
            unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    message.as_ptr(),
                    title.as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        std::process::exit(1);
    }
}

fn run() -> Result<(), zgui::Error> {
    #[cfg(target_os = "windows")]
    let _dispatcher = okcolor_app::windows_dispatcher::DispatcherRuntime::new()
        .inspect_err(|error| eprintln!("Advanced Color dispatcher unavailable: {error}"))
        .ok();
    let mut startup = StartupState::open(state_directory());
    let trace = std::env::var_os("OKCOLOR_TRACE_FRAMES").is_some();
    let display = okcolor_app::display::DisplayBridge::default();
    let renderer = graphics::factory_with_display(display.clone());
    #[cfg(target_os = "windows")]
    let caption = okcolor_app::windows_caption::CaptionBridge::new(trace);
    #[cfg(target_os = "windows")]
    let renderer = caption.wrap_renderer(renderer);
    let (width, height) = startup
        .state
        .tools
        .window
        .map(|w| (w.width, w.height))
        .unwrap_or((330.0, 582.0));
    let mut application = app()
        .with_application_id("io.github.sheetau.OkColorPicker")
        .with_title("OkColorPicker")
        .with_level(startup.state.tools.window_level())
        .with_size(width, height)
        .with_min_size(330.0, 64.0)
        .with_decorations(if cfg!(target_os = "windows") {
            Decorations::Full
        } else {
            Decorations::NoTitleBar
        })
        // The composition surface has alpha; the application scene remains opaque.
        .with_transparent(true)
        .with_renderer(renderer)
        .with_fonts(fonts::load())
        .with_stylesheet(ui::STYLE);
    #[cfg(target_os = "windows")]
    {
        application = application.with_probe(Rc::new(caption.clone()));
    }
    #[cfg(not(target_os = "windows"))]
    if trace {
        application = application.with_probe(Rc::new(FrameCount::default()));
    }
    application.run(move || {
        let model = PickerModel::restore(startup.state.clone());
        model.companion.bind(model);
        display.bind(model.display);
        model.storage_message.set(startup.warning.clone());
        if let Some(store) = startup.store.take() {
            autosave::bind(model, store, startup.state.clone());
        }
        let refs = PickerRefs::default();
        #[cfg(target_os = "windows")]
        {
            caption.provide_window();
            caption.bind_controls([refs.minimize, refs.maximize, refs.close], refs.caption);
        }
        view! { Picker(model=model,refs=refs) }
    })
}
