//! Invalidate display discovery on native window and display-setting changes.
use crate::windows_color_events::ColorEvents;
use crate::windows_display::{self, DisplayColor};
use raw_window_handle::RawWindowHandle;
use std::{
    cell::Cell,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
use zgui::platform::GpuSurface;

const SUBCLASS_ID: usize = 0x4F4B4344;
struct Invalidation {
    surface: Arc<dyn GpuSurface>,
    dirty: AtomicBool,
    active: AtomicBool,
}

impl Invalidation {
    fn notify(&self) {
        if self.active.load(Ordering::Acquire) {
            self.dirty.store(true, Ordering::Release);
            self.surface.request_redraw();
        }
    }
}

struct State {
    invalidation: Arc<Invalidation>,
    alive: Cell<bool>,
}

pub(crate) struct DisplayWatch {
    hwnd: HWND,
    state: Box<State>,
    color: Option<DisplayColor>,
    events: Option<ColorEvents>,
}

impl DisplayWatch {
    pub fn new(surface: Arc<dyn GpuSurface>) -> io::Result<Self> {
        let handle = surface.window_handle().map_err(io::Error::other)?;
        let RawWindowHandle::Win32(native) = handle.as_raw() else {
            return Err(io::Error::other("Expected a Win32 display window"));
        };
        let hwnd = native.hwnd.get() as HWND;
        let state = Box::new(State {
            invalidation: Arc::new(Invalidation {
                surface,
                dirty: AtomicBool::new(true),
                active: AtomicBool::new(true),
            }),
            alive: Cell::new(false),
        });
        // SAFETY: the surface retains the HWND. The callback and its Box live on
        // this window's UI thread until subclass removal, including during drop.
        unsafe {
            if GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) != GetCurrentThreadId() {
                return Err(io::Error::other(
                    "Display watcher requires the window thread",
                ));
            }
            if SetWindowSubclass(
                hwnd,
                Some(display_proc),
                SUBCLASS_ID,
                state.as_ref() as *const State as usize,
            ) == 0
            {
                return Err(io::Error::other("Cannot install display callback"));
            }
        }
        state.alive.set(true);
        let invalidation = state.invalidation.clone();
        // SAFETY: the subclass above verifies UI-thread ownership and retains
        // the HWND. The subscription is revoked before that state is released.
        let events = unsafe {
            ColorEvents::new(windows::Win32::Foundation::HWND(hwnd), move || {
                invalidation.notify();
            })
        }
        .inspect_err(|error| {
            eprintln!("Advanced Color events unavailable; using window messages: {error}");
        })
        .ok();
        if events.is_some() {
            eprintln!("Advanced Color notifications subscribed");
        }
        let mut watcher = Self {
            hwnd,
            state,
            color: None,
            events,
        };
        watcher.refresh();
        Ok(watcher)
    }

    pub fn refresh(&mut self) -> bool {
        if !self.state.invalidation.dirty.swap(false, Ordering::AcqRel) {
            return false;
        }
        let next = match windows_display::query(self.state.invalidation.surface.as_ref()) {
            Ok(color) => Some(color),
            Err(error) => {
                eprintln!("Display capabilities unavailable: {error}");
                None
            }
        };
        let changed = next != self.color;
        if changed {
            eprintln!("Display capabilities: {next:?}");
        }
        self.color = next;
        changed
    }

    pub fn white_scale(&self) -> Option<f32> {
        self.color.as_ref().and_then(DisplayColor::white_scale)
    }

    pub fn mode(&self) -> crate::display::DisplayMode {
        use crate::{display::DisplayMode, windows_display::ColorMode};
        match self.color.as_ref().map(|color| color.mode) {
            Some(ColorMode::Sdr) => DisplayMode::Sdr,
            Some(ColorMode::WideColor) => DisplayMode::WideColor,
            Some(ColorMode::Hdr) => DisplayMode::Hdr,
            _ => DisplayMode::Unknown,
        }
    }
}

impl Drop for DisplayWatch {
    fn drop(&mut self) {
        self.state
            .invalidation
            .active
            .store(false, Ordering::Release);
        self.events.take();
        if self.state.alive.replace(false) {
            // SAFETY: detach before the callback's boxed state and retained HWND drop.
            unsafe {
                RemoveWindowSubclass(self.hwnd, Some(display_proc), SUBCLASS_ID);
            }
        }
    }
}

unsafe extern "system" fn display_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: DisplayWatch retains this Box until removal on the same UI thread.
    // No capability query or GPU work runs inside a callback.
    unsafe {
        let state = &*(data as *const State);
        if message == WM_NCDESTROY {
            state.alive.set(false);
            state.invalidation.active.store(false, Ordering::Release);
            RemoveWindowSubclass(hwnd, Some(display_proc), SUBCLASS_ID);
        } else if matches!(
            message,
            WM_WINDOWPOSCHANGED | WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED
        ) {
            state.invalidation.notify();
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}
