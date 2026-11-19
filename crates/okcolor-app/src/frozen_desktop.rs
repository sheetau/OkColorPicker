//! Physical-pixel snapshots for the desktop eyedropper. Ownership is scoped to one Alt hold.
use crate::windows_display::{ColorMode, query_monitor};
use std::ptr::{null, null_mut};
use windows::Win32::Graphics::Dxgi::{DXGI_PRESENT, IDXGISwapChain1};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, SIZE},
        Graphics::Gdi::*,
        UI::{
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    w,
};

pub(super) struct FrozenDesktop(Vec<MonitorImage>);

impl FrozenDesktop {
    pub fn capture(owner: HWND) -> Option<Self> {
        unsafe extern "system" fn collect(
            monitor: HMONITOR,
            _: HDC,
            rect: *mut RECT,
            data: LPARAM,
        ) -> i32 {
            unsafe { (&mut *(data as *mut Vec<(HMONITOR, RECT)>)).push((monitor, *rect)) };
            1
        }
        unsafe {
            let mut monitors: Vec<(HMONITOR, RECT)> = Vec::new();
            if EnumDisplayMonitors(
                null_mut(),
                null(),
                Some(collect),
                &raw mut monitors as LPARAM,
            ) == 0
                || monitors.is_empty()
            {
                return None;
            }
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return None;
            }
            // Capture every monitor before showing any overlay, including mirrored/overlapping outputs.
            let images = monitors
                .into_iter()
                .map(|(monitor, bounds)| {
                    let advanced =
                        !query_monitor(monitor).is_ok_and(|info| info.mode == ColorMode::Sdr);
                    MonitorImage::capture(owner, screen, monitor, bounds, advanced)
                })
                .collect::<Option<Vec<_>>>();
            let desktop = images.and_then(|images| {
                let desktop = Self(images);
                for image in &desktop.0 {
                    image.show()?;
                }
                if desktop.0.iter().any(|image| image.advanced.is_some()) {
                    windows_sys::Win32::Graphics::Dwm::DwmFlush();
                    // The picker reports the OS's SDR reading of the same, now stationary scRGB image.
                    for image in &desktop.0 {
                        if image.advanced.is_some() {
                            image.read_pixels(screen)?;
                        }
                    }
                }
                Some(desktop)
            });
            ReleaseDC(null_mut(), screen);
            desktop
        }
    }

    pub fn pixel(&self, point: POINT) -> Option<u32> {
        let monitor = self.0.iter().find(|image| {
            let rect = image.bounds;
            point.x >= rect.left
                && point.x < rect.right
                && point.y >= rect.top
                && point.y < rect.bottom
        })?;
        let pixel = unsafe {
            GetPixel(
                monitor.dc,
                point.x - monitor.bounds.left,
                point.y - monitor.bounds.top,
            )
        };
        (pixel != CLR_INVALID).then_some(pixel)
    }
}

struct MonitorImage {
    bounds: RECT,
    advanced: Option<IDXGISwapChain1>,
    window: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}

impl MonitorImage {
    fn show(&self) -> Option<()> {
        unsafe {
            if SetWindowPos(
                self.window,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            ) == 0
            {
                return None;
            }
            if let Some(swap) = &self.advanced {
                swap.Present(1, DXGI_PRESENT::default()).ok().ok()?;
            }
        }
        Some(())
    }

    fn read_pixels(&self, screen: HDC) -> Option<()> {
        let r = self.bounds;
        (unsafe {
            BitBlt(
                self.dc,
                0,
                0,
                r.right - r.left,
                r.bottom - r.top,
                screen,
                r.left,
                r.top,
                SRCCOPY | CAPTUREBLT,
            )
        } != 0)
            .then_some(())
    }

    unsafe fn capture(
        owner: HWND,
        screen: HDC,
        monitor: HMONITOR,
        bounds: RECT,
        advanced: bool,
    ) -> Option<Self> {
        let pos = POINT {
            x: bounds.left,
            y: bounds.top,
        };
        let size = SIZE {
            cx: bounds.right - bounds.left,
            cy: bounds.bottom - bounds.top,
        };
        unsafe {
            let mut image = Self {
                bounds,
                advanced: None,
                window: null_mut(),
                dc: CreateCompatibleDC(screen),
                bitmap: CreateCompatibleBitmap(screen, size.cx, size.cy),
                previous: null_mut(),
            };
            if image.dc.is_null() || image.bitmap.is_null() {
                return None;
            }
            image.previous = SelectObject(image.dc, image.bitmap);
            if image.previous.is_null() {
                return None;
            }
            if !advanced {
                image.read_pixels(screen)?;
            }
            image.window = CreateWindowExW(
                WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | if advanced { 0 } else { WS_EX_LAYERED },
                w!("STATIC"),
                w!("OkColorPicker frozen desktop"),
                WS_POPUP,
                pos.x,
                pos.y,
                size.cx,
                size.cy,
                owner,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            // Publish the entire opaque image at 1:1 before the window becomes visible.
            if image.window.is_null()
                || SetWindowSubclass(image.window, Some(overlay_proc), 1, 0) == 0
            {
                return None;
            }
            if advanced {
                image.advanced = Some(
                    crate::frozen_hdr::freeze(
                        windows::Win32::Graphics::Gdi::HMONITOR(monitor),
                        windows::Win32::Foundation::HWND(image.window),
                        size.cx as u32,
                        size.cy as u32,
                    )
                    .ok()?,
                );
            } else if UpdateLayeredWindow(
                image.window,
                screen,
                &pos,
                &size,
                image.dc,
                &POINT::default(),
                0,
                null(),
                ULW_OPAQUE,
            ) == 0
            {
                return None;
            }
            Some(image)
        }
    }
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    w: usize,
    l: isize,
    _: usize,
    _: usize,
) -> isize {
    match msg {
        WM_NCHITTEST => HTCLIENT as isize,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        _ => unsafe { DefSubclassProc(hwnd, msg, w, l) },
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires Windows Graphics Capture and a desktop compositor"]
fn scrgb_snapshot_reproduces_a_visible_sdr_patch() {
    let _runtime = crate::windows_dispatcher::DispatcherRuntime::new().unwrap();
    use windows_sys::Win32::Graphics::Dwm::DwmFlush;
    unsafe {
        let source = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            w!("BUTTON"),
            w!("scRGB capture test"),
            WS_POPUP,
            100,
            100,
            100,
            100,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        );
        assert!(!source.is_null());
        SetLayeredWindowAttributes(source, 0, 255, LWA_ALPHA);
        ShowWindow(source, SW_SHOWNOACTIVATE);
        UpdateWindow(source);
        let dc = GetDC(source);
        let brush = CreateSolidBrush(0x47bd8b);
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            brush,
        );
        DeleteObject(brush);
        ReleaseDC(source, dc);
        GdiFlush();
        DwmFlush();
        let mut point = POINT { x: 40, y: 40 };
        ClientToScreen(source, &mut point);
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        assert_ne!(GetMonitorInfoW(monitor, &mut info), 0);
        let screen = GetDC(null_mut());
        let before = GetPixel(screen, point.x, point.y);
        assert_eq!(before, 0x47bd8b);
        let image = MonitorImage::capture(source, screen, monitor, info.rcMonitor, true)
            .expect("capture scRGB monitor");
        image.show().unwrap();
        DwmFlush();
        let after = GetPixel(screen, point.x, point.y);
        drop(image);
        ReleaseDC(null_mut(), screen);
        DestroyWindow(source);
        for shift in [0, 8, 16] {
            assert!(
                ((before >> shift) & 255).abs_diff((after >> shift) & 255) <= 1,
                "visible patch changed from {before:06x} to {after:06x}"
            );
        }
    }
}

impl Drop for MonitorImage {
    fn drop(&mut self) {
        unsafe {
            self.advanced.take();
            if !self.window.is_null() {
                DestroyWindow(self.window);
            }
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
        }
    }
}
