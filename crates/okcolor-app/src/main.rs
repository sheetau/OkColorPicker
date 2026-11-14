use okcolor_app::{
    autosave, fonts, graphics,
    model::PickerModel,
    persistence::{StartupState, state_directory},
    ui::{self, PickerProps, PickerRefs},
};
use std::{cell::Cell, rc::Rc};
use zgui::prelude::*;

#[derive(Default)]
struct FrameCount(Cell<u64>);
impl zgui::runtime::FrameProbe for FrameCount {
    fn frame_ended(&self, _window: &zgui::runtime::Window) {
        let frame = self.0.get() + 1;
        self.0.set(frame);
        eprintln!("Rendered frame {frame}");
    }
}

fn main() -> Result<(), zgui::Error> {
    let mut startup = StartupState::open(state_directory());
    let mut application = app()
        .with_application_id("io.github.sheetau.OkColorPicker")
        .with_title("OkColorPicker")
        .with_size(330.0, 560.0)
        .with_min_size(330.0, 520.0)
        .with_decorations(Decorations::NoTitleBar)
        // The composition surface has alpha; the application scene remains opaque.
        .with_transparent(true)
        .with_renderer(graphics::factory())
        .with_fonts(fonts::load())
        .with_stylesheet(ui::STYLE);
    if std::env::var_os("OKCOLOR_TRACE_FRAMES").is_some() {
        application = application.with_probe(Rc::new(FrameCount::default()));
    }
    application.run(move || {
        let model = PickerModel::restore(startup.state.clone());
        model.storage_message.set(startup.warning.clone());
        if let Some(store) = startup.store.take() {
            autosave::bind(model, store, startup.state.clone());
        }
        view! { Picker(model=model,refs=PickerRefs::default()) }
    })
}
