use std::sync::Arc;
use zgui::app::Fonts;

/// Mono is redistributable under OFL; the optional Grotesk file stays user-owned.
pub fn load() -> Fonts {
    let fonts = Fonts::system();
    fonts
        .register(
            Arc::new(include_bytes!("../assets/MartianMono.ttf").as_slice()),
            Some("Martian Mono"),
        )
        .expect("the embedded Martian Mono font must be valid");
    if let Some(path) = std::env::var_os("OKCOLOR_GROTESK_FONT") {
        match std::fs::read(path) {
            Ok(bytes) => {
                if let Err(error) = fonts.register(Arc::new(bytes), Some("Martian Grotesk")) {
                    eprintln!("Cannot load the optional Grotesk font: {error}");
                }
            }
            Err(error) => eprintln!("Cannot read the optional Grotesk font: {error}"),
        }
    }
    fonts
}
