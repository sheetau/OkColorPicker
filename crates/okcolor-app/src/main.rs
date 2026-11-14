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

fn main() -> Result<(), zgui::Error> {
    let mut startup = StartupState::open(state_directory());
    let trace = std::env::var_os("OKCOLOR_TRACE_FRAMES").is_some();
    let renderer = graphics::factory();
    #[cfg(target_os = "windows")]
    let caption = okcolor_app::windows_caption::CaptionBridge::new(trace);
    #[cfg(target_os = "windows")]
    let renderer = caption.wrap_renderer(renderer);
    let mut application = app()
        .with_application_id("io.github.sheetau.OkColorPicker")
        .with_title("OkColorPicker")
        .with_size(330.0, 560.0)
        .with_min_size(330.0, 520.0)
        .with_decorations(Decorations::NoTitleBar)
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
        model.storage_message.set(startup.warning.clone());
        if let Some(store) = startup.store.take() {
            autosave::bind(model, store, startup.state.clone());
        }
        let refs = PickerRefs::default();
        #[cfg(target_os = "windows")]
        caption.bind_maximize(refs.maximize);
        view! { Picker(model=model,refs=refs) }
    })
}
