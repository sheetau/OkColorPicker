//! Expose the painted maximize button to Windows Snap while zgui owns its click.
use raw_window_handle::RawWindowHandle;
use std::{
    cell::{Cell, RefCell},
    io,
    rc::Rc,
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    System::Threading::GetCurrentThreadId,
    UI::{
        Controls::WM_MOUSELEAVE,
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            IsWindowEnabled, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
use zgui::{
    platform::GpuSurface,
    prelude::NodeRef,
    runtime::{AppError, FrameProbe, RendererFactory, Window},
};

#[derive(Clone)]
pub struct CaptionBridge(Rc<BridgeState>);

struct BridgeState {
    button: Cell<Option<NodeRef>>,
    native: RefCell<Option<NativeCaption>>,
    frames: Cell<u64>,
    trace: bool,
}

struct NativeCaption {
    // Detach the callback before releasing the surface that owns the HWND.
    hook: Hook,
    _surface: Arc<dyn GpuSurface>,
}

impl CaptionBridge {
    pub fn new(trace: bool) -> Self {
        Self(Rc::new(BridgeState {
            button: Cell::new(None),
            native: RefCell::new(None),
            frames: Cell::new(0),
            trace,
        }))
    }

    pub fn bind_maximize(&self, button: NodeRef) {
        self.0.button.set(Some(button));
    }

    pub fn wrap_renderer(&self, mut renderer: RendererFactory) -> RendererFactory {
        let bridge = self.clone();
        Box::new(move |surface, target| {
            let gpu = surface
                .clone()
                .gpu_shared()
                .ok_or_else(|| native_error("Missing native surface"))?;
            let handle = gpu.window_handle().map_err(native_error)?;
            let RawWindowHandle::Win32(native) = handle.as_raw() else {
                return Err(native_error("Expected a Win32 caption window"));
            };
            let hwnd = native.hwnd.get() as HWND;
            let mut caption = bridge.0.native.borrow_mut();
            if let Some(caption) = caption.as_ref() {
                if caption.hook.hwnd != hwnd {
                    return Err(native_error(
                        "Use a separate caption bridge for each window",
                    ));
                }
            } else {
                // SAFETY: gpu retains the live window; the Hook checks the UI thread.
                let hook = unsafe { Hook::install(hwnd) }.map_err(native_error)?;
                *caption = Some(NativeCaption {
                    hook,
                    _surface: gpu,
                });
            }
            drop(caption);
            renderer(surface, target)
        })
    }
}

fn native_error(error: impl std::fmt::Display) -> AppError {
    AppError::Platform(zgui::platform::PlatformError::Backend(format!(
        "Native caption: {error}"
    )))
}

impl FrameProbe for CaptionBridge {
    fn frame_ended(&self, _: &Window) {
        if let Some(native) = self.0.native.borrow().as_ref() {
            let bounds = self
                .0
                .button
                .get()
                .and_then(|button| button.window_bounds());
            let rect = bounds.map(|bounds| {
                [
                    bounds.origin.x.0,
                    bounds.origin.y.0,
                    bounds.origin.x.0 + bounds.size.width.0,
                    bounds.origin.y.0 + bounds.size.height.0,
                ]
            });
            native.hook.update(rect);
        }
        if self.0.trace {
            let frames = self.0.frames.get() + 1;
            self.0.frames.set(frames);
            eprintln!("Rendered frame {frames}");
        }
    }
}

#[derive(Clone, Copy)]
struct Layout {
    rect: [f32; 4],
    extent: [i32; 2],
    dpi: u32,
    maximized: bool,
}

#[derive(Default)]
struct State {
    layout: Cell<Option<Layout>>,
    hovered: Cell<bool>,
    pressed: Cell<bool>,
    alive: Cell<bool>,
}

struct Hook {
    hwnd: HWND,
    state: Box<State>,
}

const SUBCLASS_ID: usize = 1;

impl Hook {
    /// The caller retains this HWND and drops the hook on its owning UI thread.
    unsafe fn install(hwnd: HWND) -> io::Result<Self> {
        // SAFETY: caller retains the HWND; reject cross-thread subclassing.
        unsafe {
            if GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) != GetCurrentThreadId() {
                return Err(io::Error::other(
                    "Caption setup requires the window's UI thread",
                ));
            }
            let state = Box::new(State::default());
            if SetWindowSubclass(
                hwnd,
                Some(caption_proc),
                SUBCLASS_ID,
                state.as_ref() as *const State as usize,
            ) == 0
            {
                return Err(io::Error::other("Cannot install the caption callback"));
            }
            state.alive.set(true);
            Ok(Self { hwnd, state })
        }
    }

    fn update(&self, bounds: Option<[f32; 4]>) {
        // Never use guessed button coordinates while layout is missing or stale.
        let layout = bounds
            .filter(|b| b.iter().all(|v| v.is_finite()))
            .filter(|b| b[2] > b[0] && b[3] > b[1])
            .and_then(|bounds| {
                // SAFETY: Hook's owner retains the same-thread native window.
                unsafe { window_state(self.hwnd) }.map(|(extent, dpi, maximized)| Layout {
                    rect: bounds,
                    extent,
                    dpi,
                    maximized,
                })
            });
        self.state.layout.set(layout);
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        if self.state.alive.replace(false) {
            // SAFETY: owner retains the HWND on this thread; detach before freeing State.
            unsafe {
                RemoveWindowSubclass(self.hwnd, Some(caption_proc), SUBCLASS_ID);
            }
        }
    }
}

unsafe fn window_state(hwnd: HWND) -> Option<([i32; 2], u32, bool)> {
    let mut client = RECT::default();
    // SAFETY: synchronous queries on the retained HWND, no pointers escape.
    unsafe {
        (GetClientRect(hwnd, &mut client) != 0).then(|| {
            (
                [client.right - client.left, client.bottom - client.top],
                GetDpiForWindow(hwnd),
                IsZoomed(hwnd) != 0,
            )
        })
    }
}

unsafe fn hit(hwnd: HWND, state: &State, point: POINT) -> bool {
    let Some(layout) = state.layout.get() else {
        return false;
    };
    // SAFETY: synchronous scalar queries on the callback's HWND.
    unsafe {
        if IsWindowEnabled(hwnd) == 0
            || GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_MAXIMIZEBOX == 0
            || window_state(hwnd) != Some((layout.extent, layout.dpi, layout.maximized))
        {
            return false;
        }
    }
    point.x as f32 >= layout.rect[0]
        && (point.x as f32) < layout.rect[2]
        && point.y as f32 >= layout.rect[1]
        && (point.y as f32) < layout.rect[3]
}

fn unpack_point(lparam: LPARAM) -> POINT {
    POINT {
        x: lparam as i16 as i32,
        y: (lparam >> 16) as i16 as i32,
    }
}

fn pack_point(point: POINT) -> LPARAM {
    (point.x as u16 as u32 | ((point.y as u16 as u32) << 16)) as LPARAM
}

unsafe extern "system" fn caption_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: Hook retains the boxed State until subclass removal. No mutable
    // borrow spans a forwarded message; callbacks may reenter through winit.
    unsafe {
        let state = &*(data as *const State);
        if message == WM_NCDESTROY {
            state.alive.set(false);
            RemoveWindowSubclass(hwnd, Some(caption_proc), SUBCLASS_ID);
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
        if matches!(message, WM_SIZE | WM_DPICHANGED) {
            state.layout.set(None);
        }
        if matches!(
            message,
            WM_CANCELMODE | WM_CAPTURECHANGED | WM_KILLFOCUS | WM_LBUTTONUP
        ) {
            state.pressed.set(false);
        }
        if message == WM_MOUSEMOVE {
            // A later non-client leave must not clear a newer client hover.
            state.hovered.set(false);
        }
        if message == WM_NCMOUSELEAVE && state.hovered.replace(false) {
            DefSubclassProc(hwnd, WM_MOUSELEAVE, 0, 0);
        }
        if matches!(
            message,
            WM_NCHITTEST | WM_NCMOUSEMOVE | WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK | WM_NCLBUTTONUP
        ) {
            let mut point = unpack_point(lparam);
            if ScreenToClient(hwnd, &mut point) != 0 {
                let over_button = hit(hwnd, state, point);
                if message == WM_NCHITTEST && over_button {
                    return HTMAXBUTTON as LRESULT;
                }
                if message == WM_NCMOUSEMOVE && over_button {
                    if !state.hovered.replace(true) {
                        TrackMouseEvent(&mut TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_NONCLIENT | TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        });
                    }
                    DefSubclassProc(hwnd, WM_MOUSEMOVE, 0, pack_point(point));
                    // Windows still receives native hover for the Snap flyout.
                }
                if over_button && matches!(message, WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK) {
                    state.pressed.set(true);
                    DefSubclassProc(hwnd, WM_MOUSEMOVE, 0, pack_point(point));
                    // Winit captures the mouse and zgui runs the existing button
                    // gesture. Do not also invoke the default native maximize.
                    return DefSubclassProc(hwnd, WM_LBUTTONDOWN, 0, pack_point(point));
                }
                if message == WM_NCLBUTTONUP && state.pressed.replace(false) {
                    DefSubclassProc(hwnd, WM_MOUSEMOVE, 0, pack_point(point));
                    return DefSubclassProc(hwnd, WM_LBUTTONUP, 0, pack_point(point));
                }
            }
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests;
