//! Win32 owns caption gestures; zgui paints the same Segoe controls as towavue.
use crate::window_frame::CaptionVisual;
use raw_window_handle::RawWindowHandle;
use std::{
    cell::{Cell, RefCell},
    io,
    rc::Rc,
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
        },
        Gdi::ScreenToClient,
    },
    System::Threading::GetCurrentThreadId,
    UI::{
        Controls::MARGINS,
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        Input::KeyboardAndMouse::{
            GetCapture, IsWindowEnabled, ReleaseCapture, SetCapture, TME_LEAVE, TME_NONCLIENT,
            TRACKMOUSEEVENT, TrackMouseEvent,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
use zgui::{
    platform::GpuSurface,
    prelude::{GetUntracked, NodeRef, RwSignal, Set},
    runtime::{AppError, FrameProbe, RendererFactory, Window},
};

#[derive(Clone)]
pub struct CaptionBridge(Rc<BridgeState>);

struct BridgeState {
    buttons: Cell<Option<[NodeRef; 3]>>,
    visual: Cell<Option<RwSignal<CaptionVisual>>>,
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
            buttons: Cell::new(None),
            visual: Cell::new(None),
            native: RefCell::new(None),
            frames: Cell::new(0),
            trace,
        }))
    }

    /// Make this window's native surface available to the desktop sampling tool.
    pub fn provide_window(&self) {
        zgui::prelude::provide_context(zgui::prelude::StoredValue::new_local(self.clone()));
    }

    pub(crate) fn hwnd(&self) -> Option<HWND> {
        self.0
            .native
            .borrow()
            .as_ref()
            .map(|native| native.hook.hwnd)
    }

    pub fn bind_controls(&self, buttons: [NodeRef; 3], visual: RwSignal<CaptionVisual>) {
        self.0.buttons.set(Some(buttons));
        self.0.visual.set(Some(visual));
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
            let rects = self
                .0
                .buttons
                .get()
                .map(|buttons| {
                    buttons.map(|button| {
                        button.window_bounds().map(|b| {
                            [
                                b.origin.x.0,
                                b.origin.y.0,
                                b.origin.x.0 + b.size.width.0,
                                b.origin.y.0 + b.size.height.0,
                            ]
                        })
                    })
                })
                .unwrap_or([None; 3]);
            native.hook.state.visual.set(self.0.visual.get());
            native.hook.update(rects);
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
    rects: [Option<[f32; 4]>; 3],
    extent: [i32; 2],
    dpi: u32,
    maximized: bool,
}

#[derive(Default)]
struct State {
    layout: Cell<Option<Layout>>,
    hovered: Cell<Option<usize>>,
    pressed: Cell<Option<usize>>,
    visual: Cell<Option<RwSignal<CaptionVisual>>>,
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
            let mut before = RECT::default();
            GetClientRect(hwnd, &mut before);
            let dark = 1_i32;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                (&dark as *const i32).cast(),
                4,
            );
            let black = 0_u32;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_CAPTION_COLOR as u32,
                (&black as *const u32).cast(),
                4,
            );
            let corners = DWMWCP_ROUND;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&corners as *const i32).cast(),
                4,
            );
            extend_frame(hwnd);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            // Removing the OS titlebar increases client height. Keep the requested
            // initial client size rather than changing the application's layout.
            if IsZoomed(hwnd) == 0 {
                let mut after = RECT::default();
                let mut outer = RECT::default();
                GetClientRect(hwnd, &mut after);
                GetWindowRect(hwnd, &mut outer);
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    outer.right - outer.left + before.right - after.right,
                    outer.bottom - outer.top + before.bottom - after.bottom,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            Ok(Self { hwnd, state })
        }
    }

    fn update(&self, bounds: [Option<[f32; 4]>; 3]) {
        // Never use guessed button coordinates while layout is missing or stale.
        let rects = bounds
            .map(|b| b.filter(|b| b.iter().all(|v| v.is_finite()) && b[2] > b[0] && b[3] > b[1]));
        // SAFETY: Hook's owner retains the same-thread native window.
        let layout = unsafe { window_state(self.hwnd) }.map(|(extent, dpi, maximized)| Layout {
            rects,
            extent,
            dpi,
            maximized,
        });
        self.state.layout.set(layout);
        self.state.publish(self.hwnd);
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        if self.state.alive.replace(false) {
            self.state.clear(self.hwnd);
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

unsafe fn hit(hwnd: HWND, state: &State, point: POINT) -> Option<usize> {
    let layout = state.layout.get()?;
    // SAFETY: synchronous scalar queries on the callback's HWND.
    unsafe {
        if window_state(hwnd) != Some((layout.extent, layout.dpi, layout.maximized)) {
            return None;
        }
    }
    layout.rects.iter().enumerate().find_map(|(index, rect)| {
        let rect = rect.as_ref()?;
        (enabled(hwnd, index)
            && point.x as f32 >= rect[0]
            && (point.x as f32) < rect[2]
            && point.y as f32 >= rect[1]
            && (point.y as f32) < rect[3])
            .then_some(index)
    })
}

fn enabled(hwnd: HWND, index: usize) -> bool {
    // SAFETY: all callers retain the same-thread HWND through the hook callback.
    unsafe {
        if IsWindowEnabled(hwnd) == 0 {
            return false;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        match index {
            0 => style & WS_MINIMIZEBOX != 0,
            1 => style & WS_MAXIMIZEBOX != 0,
            2 => GetClassLongPtrW(hwnd, GCL_STYLE) & CS_NOCLOSE as usize == 0,
            _ => false,
        }
    }
}

impl State {
    fn publish(&self, hwnd: HWND) {
        if let Some(signal) = self.visual.get() {
            // SAFETY: scalar query of the hook's retained HWND.
            let value = CaptionVisual {
                hovered: self.hovered.get(),
                pressed: self.pressed.get(),
                enabled: std::array::from_fn(|i| enabled(hwnd, i)),
                maximized: unsafe { IsZoomed(hwnd) != 0 },
            };
            if signal.try_get_untracked().is_some_and(|old| old != value) {
                signal.try_set(value);
            }
        }
    }
    fn clear(&self, hwnd: HWND) {
        self.hovered.set(None);
        if self.pressed.take().is_some() {
            // SAFETY: release only this window's capture; callbacks may reenter through Cells.
            unsafe {
                if GetCapture() == hwnd {
                    ReleaseCapture();
                }
            }
        }
        self.publish(hwnd);
    }
}

fn unpack_point(lparam: LPARAM) -> POINT {
    POINT {
        x: lparam as i16 as i32,
        y: (lparam >> 16) as i16 as i32,
    }
}

#[cfg(test)]
fn pack_point(point: POINT) -> LPARAM {
    (point.x as u16 as u32 | ((point.y as u16 as u32) << 16)) as LPARAM
}

unsafe fn extend_frame(hwnd: HWND) {
    // SAFETY: caller retains the window; DWM copies margins synchronously.
    unsafe {
        let margins = MARGINS {
            cyTopHeight: (32 * GetDpiForWindow(hwnd) / 96) as i32,
            ..Default::default()
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

unsafe extern "system" fn caption_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: Hook retains State until subclass removal. Only Cells span reentrant
    // native calls; actions are queued after capture is released.
    unsafe {
        let state = &*(data as *const State);
        if message == WM_NCCALCSIZE && wparam != 0 {
            let params = &mut *(lparam as *mut NCCALCSIZE_PARAMS);
            let top = params.rgrc[0].top;
            // Keep the OS's left/right/bottom frame. Only replace its titlebar;
            // the maximized top resize margin stays outside the visible client.
            DefSubclassProc(hwnd, message, wparam, lparam);
            let margin = if IsZoomed(hwnd) != 0 {
                let dpi = GetDpiForWindow(hwnd);
                GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
                    + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
            } else {
                0
            };
            (*(lparam as *mut NCCALCSIZE_PARAMS)).rgrc[0].top = top + margin;
            return 0;
        }
        if matches!(
            message,
            WM_ACTIVATE | WM_DWMCOMPOSITIONCHANGED | WM_DPICHANGED
        ) {
            extend_frame(hwnd);
        }
        if message == WM_NCDESTROY {
            state.clear(hwnd);
            state.alive.set(false);
            RemoveWindowSubclass(hwnd, Some(caption_proc), SUBCLASS_ID);
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
        if matches!(message, WM_SIZE | WM_DPICHANGED) {
            state.layout.set(None);
        }
        if matches!(
            message,
            WM_CANCELMODE | WM_CAPTURECHANGED | WM_KILLFOCUS | WM_SIZE | WM_DPICHANGED | WM_ENABLE
        ) {
            state.clear(hwnd);
        }
        if message == WM_NCMOUSELEAVE {
            state.hovered.set(None);
            state.publish(hwnd);
        }
        if matches!(
            message,
            WM_NCHITTEST
                | WM_NCMOUSEMOVE
                | WM_NCLBUTTONDOWN
                | WM_NCLBUTTONDBLCLK
                | WM_NCLBUTTONUP
                | WM_MOUSEMOVE
                | WM_LBUTTONUP
        ) {
            let mut point = unpack_point(lparam);
            if matches!(message, WM_MOUSEMOVE | WM_LBUTTONUP)
                || ScreenToClient(hwnd, &mut point) != 0
            {
                let over = hit(hwnd, state, point);
                if message == WM_NCHITTEST
                    && let Some(index) = over
                {
                    return [HTMINBUTTON, HTMAXBUTTON, HTCLOSE][index] as LRESULT;
                }
                if message == WM_NCHITTEST {
                    let native = DefSubclassProc(hwnd, message, wparam, lparam);
                    if !matches!(
                        native as u32,
                        HTCLIENT | HTCAPTION | HTMINBUTTON | HTMAXBUTTON | HTCLOSE
                    ) {
                        return native;
                    }
                    if IsZoomed(hwnd) == 0
                        && point.y >= 0
                        && point.y < (4 * GetDpiForWindow(hwnd) / 96) as i32
                    {
                        return HTTOP as LRESULT;
                    }
                    // The titlebar's mounted controls handle client input, including
                    // the visible topmost pixel of a maximized window.
                    return HTCLIENT as LRESULT;
                }
                if message == WM_NCMOUSEMOVE {
                    state.hovered.set(over);
                    state.publish(hwnd);
                    TrackMouseEvent(&mut TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_NONCLIENT | TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    });
                    // Default non-client processing preserves Windows Snap hover.
                }
                if over.is_some() && matches!(message, WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK) {
                    state.pressed.set(over);
                    state.hovered.set(over);
                    SetForegroundWindow(hwnd);
                    SetCapture(hwnd);
                    state.publish(hwnd);
                    return 0;
                }
                if message == WM_MOUSEMOVE {
                    let captured = state.pressed.get().is_some();
                    state.hovered.set(if captured { over } else { None });
                    state.publish(hwnd);
                    if captured {
                        return 0;
                    }
                }
                if matches!(message, WM_NCLBUTTONUP | WM_LBUTTONUP)
                    && let Some(pressed) = state.pressed.take()
                {
                    if GetCapture() == hwnd {
                        ReleaseCapture();
                    }
                    state.hovered.set(over);
                    state.publish(hwnd);
                    if over == Some(pressed) && enabled(hwnd, pressed) {
                        let command = match pressed {
                            0 => SC_MINIMIZE,
                            1 if IsZoomed(hwnd) != 0 => SC_RESTORE,
                            1 => SC_MAXIMIZE,
                            _ => SC_CLOSE,
                        };
                        if PostMessageW(hwnd, WM_SYSCOMMAND, command as WPARAM, 0) == 0 {
                            eprintln!(
                                "Cannot queue caption action: {}",
                                io::Error::last_os_error()
                            );
                        }
                    }
                    return 0;
                }
            }
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests;
