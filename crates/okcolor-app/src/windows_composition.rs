//! Verify the native window policy before creating a DirectComposition swap chain.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::io;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_NOREDIRECTIONBITMAP,
};

pub(crate) fn verify(window: &(impl HasWindowHandle + ?Sized)) -> io::Result<()> {
    let handle = window.window_handle().map_err(io::Error::other)?;
    let RawWindowHandle::Win32(native) = handle.as_raw() else {
        return Err(io::Error::other("Expected a Win32 presentation window"));
    };
    // SAFETY: the borrowed window handle remains alive for this read-only call.
    let style = unsafe { GetWindowLongPtrW(native.hwnd.get() as _, GWL_EXSTYLE) };
    if style & WS_EX_NOREDIRECTIONBITMAP as isize == 0 {
        return Err(io::Error::other(
            "DirectComposition requires WS_EX_NOREDIRECTIONBITMAP at window creation",
        ));
    }
    eprintln!("Composition window: WS_EX_NOREDIRECTIONBITMAP verified");
    Ok(())
}
