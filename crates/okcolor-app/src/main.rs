use okcolor_app::{fonts, graphics};
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

#[component]
fn Probe() -> impl IntoView {
    let (updates, set_updates) = signal(0_u32);
    view! {
        column(class = "probe") {
            text(class = "title") {"OkColorPicker"}
            text {"Windows rendering probe"}
            text {{move || format!("Updates: {}", updates.get())}}
            control(
                tabindex = Focus::Sequential,
                a11y:label = "Update probe",
                on:click = move |_| set_updates.update(|value| *value += 1)
            ) {"Update"}
        }
    }
}

const STYLE: &str = css!(
    ":root { background-color: #18181b; color: #eeeeef; font-family: sans-serif; }
    .probe { padding: 24px; gap: 16px; }
    .title { font-size: 24px; }
    control { padding: 8px 16px; background-color: #564574; border-radius: 4px; }
    control:hover { background-color: #69558b; }"
);

fn main() -> Result<(), zgui::Error> {
    eprintln!("Starting native probe");
    let application = app()
        .with_application_id("io.github.sheetau.OkColorPicker")
        .with_title("OkColorPicker — rendering probe")
        .with_size(420.0, 360.0)
        // Request premultiplied composition while keeping the CSS scene opaque.
        .with_transparent(true)
        .with_renderer(graphics::factory())
        .with_fonts(fonts::load())
        .with_stylesheet(STYLE)
        .with_probe(Rc::new(FrameCount::default()));
    eprintln!("Application configured");
    let handler = application.into_handler(|| view! { Probe() })?;
    eprintln!("Runtime initialized");
    handler.drive(zgui::app::desktop())
}
