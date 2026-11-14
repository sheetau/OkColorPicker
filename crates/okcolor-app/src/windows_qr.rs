//! One-shot, in-memory desktop QR capture on the companion worker thread.
use okcolor_csp::{
    Pairing,
    qr::{Scan, ScanError, pixel_count},
};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{LPARAM, RECT},
    Graphics::Gdi::*,
    UI::HiDpi::{
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        SetThreadDpiAwarenessContext,
    },
};

struct DpiScope(DPI_AWARENESS_CONTEXT);
impl Drop for DpiScope {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}
struct Capture {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.memory.is_null() {
                DeleteDC(self.memory);
            }
            if !self.screen.is_null() {
                ReleaseDC(null_mut(), self.screen);
            }
        }
    }
}

unsafe extern "system" fn collect_monitor(
    _: HMONITOR,
    _: HDC,
    rect: *mut RECT,
    data: LPARAM,
) -> i32 {
    // EnumDisplayMonitors invokes this synchronously while `rects` is alive.
    let rects = unsafe { &mut *(data as *mut Vec<RECT>) };
    if rects.len() >= 16 || rect.is_null() {
        return 0;
    }
    rects.push(unsafe { *rect });
    1
}

pub fn scan(cancelled: &dyn Fn() -> bool) -> Result<Pairing, ScanError> {
    let old_dpi =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if old_dpi.is_null() {
        return Err(ScanError::CaptureUnavailable);
    }
    let _dpi = DpiScope(old_dpi);
    let mut rects = Vec::<RECT>::with_capacity(16);
    if unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect_monitor),
            &mut rects as *mut _ as LPARAM,
        )
    } == 0
        || rects.is_empty()
    {
        return Err(ScanError::CaptureUnavailable);
    }
    let mut result = Scan::default();
    for rect in rects {
        if cancelled() {
            return Err(ScanError::Cancelled);
        }
        let (width, height) = dimensions(rect)?;
        let pixels = capture(rect, width, height)?;
        if cancelled() {
            return Err(ScanError::Cancelled);
        }
        result.add_greyscale(width, height, &pixels)?;
    }
    if cancelled() {
        return Err(ScanError::Cancelled);
    }
    result.finish()
}

fn dimensions(rect: RECT) -> Result<(usize, usize), ScanError> {
    let width = usize::try_from(i64::from(rect.right) - i64::from(rect.left))
        .map_err(|_| ScanError::InvalidImage)?;
    let height = usize::try_from(i64::from(rect.bottom) - i64::from(rect.top))
        .map_err(|_| ScanError::InvalidImage)?;
    pixel_count(width, height)?;
    Ok((width, height))
}

fn capture(rect: RECT, width: usize, height: usize) -> Result<Vec<u8>, ScanError> {
    let mut resources = Capture {
        screen: unsafe { GetDC(null_mut()) },
        memory: null_mut(),
        bitmap: null_mut(),
        previous: null_mut(),
    };
    if resources.screen.is_null() {
        return Err(ScanError::CaptureUnavailable);
    }
    resources.memory = unsafe { CreateCompatibleDC(resources.screen) };
    if resources.memory.is_null() {
        return Err(ScanError::CaptureUnavailable);
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = null_mut();
    resources.bitmap = unsafe {
        CreateDIBSection(
            resources.screen,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        )
    };
    if resources.bitmap.is_null() || bits.is_null() {
        return Err(ScanError::CaptureUnavailable);
    }
    resources.previous = unsafe { SelectObject(resources.memory, resources.bitmap) };
    if resources.previous.is_null() || resources.previous as isize == -1 {
        resources.previous = null_mut();
        return Err(ScanError::CaptureUnavailable);
    }
    if unsafe {
        BitBlt(
            resources.memory,
            0,
            0,
            width as i32,
            height as i32,
            resources.screen,
            rect.left,
            rect.top,
            SRCCOPY | CAPTUREBLT,
        )
    } == 0
        || unsafe { GdiFlush() } == 0
    {
        return Err(ScanError::CaptureUnavailable);
    }
    // Top-down 32-bit BI_RGB has width * 4 stride; alpha is unspecified by GDI.
    let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), width * height * 4) };
    Ok(greyscale(bgra))
}

fn greyscale(bgra: &[u8]) -> Vec<u8> {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            ((u32::from(pixel[2]) * 77
                + u32::from(pixel[1]) * 150
                + u32::from(pixel[0]) * 29
                + 128)
                >> 8) as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_monitor_coordinates_and_bgra_stride_are_preserved() {
        assert_eq!(
            dimensions(RECT {
                left: -3840,
                top: -400,
                right: 0,
                bottom: 1760
            })
            .unwrap(),
            (3840, 2160)
        );
        assert!(
            dimensions(RECT {
                left: i32::MIN,
                top: 0,
                right: i32::MAX,
                bottom: 1
            })
            .is_err()
        );
        assert_eq!(
            greyscale(&[
                0, 0, 255, 0, 0, 255, 0, 255, 255, 0, 0, 128, 255, 255, 255, 0
            ]),
            vec![77, 149, 29, 255]
        );
    }
}
