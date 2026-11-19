use std::sync::Arc;
use zgui::app::Fonts;

pub fn load() -> Fonts {
    let fonts = Fonts::system();
    if let Some(bytes) = caption_font() {
        fonts
            .register(bytes.clone(), Some("Windows Caption"))
            .expect("installed caption font must be valid");
    }
    fonts
        .register(
            Arc::new(include_bytes!("../assets/MartianMono.ttf").as_slice()),
            Some("Martian Mono"),
        )
        .expect("the embedded Martian Mono font must be valid");
    fonts
        .register(
            Arc::new(include_bytes!("../assets/MartianGrotesk.ttf").as_slice()),
            Some("Martian Grotesk"),
        )
        .expect("the embedded Martian Grotesk font must be valid");
    fonts
}

/// Read the same installed Segoe family as towavue; never bundle or substitute icons.
pub(crate) fn caption_font() -> Option<&'static Arc<Vec<u8>>> {
    static CAPTION: std::sync::OnceLock<Option<Arc<Vec<u8>>>> = std::sync::OnceLock::new();
    CAPTION
        .get_or_init(|| {
            #[cfg(target_os = "windows")]
            {
                use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
                #[link(name = "ntdll")]
                unsafe extern "system" {
                    fn RtlGetVersion(version: *mut OSVERSIONINFOW) -> i32;
                }
                let mut version = OSVERSIONINFOW {
                    dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
                    ..Default::default()
                };
                // SAFETY: the OS fills this initialized SDK structure synchronously.
                let fluent =
                    unsafe { RtlGetVersion(&mut version) } >= 0 && version.dwBuildNumber >= 22000;
                let fonts = std::path::PathBuf::from(std::env::var_os("WINDIR")?).join("Fonts");
                std::fs::read(fonts.join(if fluent {
                    "SegoeIcons.ttf"
                } else {
                    "segmdl2.ttf"
                }))
                .or_else(|_| std::fs::read(fonts.join("segmdl2.ttf")))
                .ok()
                .map(Arc::new)
            }
            #[cfg(not(target_os = "windows"))]
            {
                None
            }
        })
        .as_ref()
}
