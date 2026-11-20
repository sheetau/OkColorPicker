use crate::model::PickerModel;
use zgui::prelude::*;

#[component]
pub(crate) fn About(model: PickerModel) -> impl IntoView {
    let link = move |label: &'static str, url: Option<&'static str>| {
        view! {control(class="about-link",tabindex=Focus::Sequential,a11y:label=label,
        on:click=move |_| { if let Err(message) = open(url) { model.message.set(message); } }) { {label} }}
    };
    view! {column(class="about",a11y:label="About OkColorPicker") {
        text(class="about-version") { {concat!("OkColorPicker / Version ", env!("CARGO_PKG_VERSION"))} }
        row(class="about-author") { text {"OKLCH color picker created by"} {link("sheeta",Some("https://linktr.ee/sheetau"))} }
        {link("GitHub",Some("https://github.com/sheetau/OkColorPicker"))}
        text {"Apache-2.0. Provided without warranty."}
        {link("Licenses and sources",None)}
    }}
}

#[cfg(windows)]
fn open(url: Option<&str>) -> Result<(), String> {
    use std::{ffi::OsString, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let target = match url {
        Some(url) => OsString::from(url),
        None => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let notices = exe.with_file_name("THIRD_PARTY_NOTICES.html");
            if !notices.is_file() {
                return Err("Licenses and sources are included in the Windows package".into());
            }
            notices.into_os_string()
        }
    };
    let wide: Vec<u16> = target.encode_wide().chain(Some(0)).collect();
    // SAFETY: the fixed URL or notice path lives through this synchronous call.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            ptr::null(),
            wide.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        return Err("Could not open the About link".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn open(_: Option<&str>) -> Result<(), String> {
    Err("Opening About links requires Windows".into())
}
