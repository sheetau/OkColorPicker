//! A transient desktop eyedropper. A Windows mouse hook consumes the sampling
//! click through button-up; an independent preview follows the cursor across displays.
use crate::{model::PickerModel, ui::PickerRefs};
use zgui::prelude::*;

#[component]
pub(crate) fn Eyedropper(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    #[cfg(windows)]
    if let Some(owner) =
        use_context::<StoredValue<crate::windows_caption::CaptionBridge, LocalStorage>>()
    {
        let hwnd = owner.with_value(|bridge| bridge.hwnd());
        let finish = move |result: Result<Option<u32>, ()>| {
            match result {
                Ok(Some(pixel)) => {
                    let rgb = [pixel as u8, (pixel >> 8) as u8, (pixel >> 16) as u8];
                    if let Ok(color) = okcolor_core::Oklch::from_rgb(
                        rgb.map(|v| f64::from(v) / 255.0),
                        okcolor_core::Gamut::Srgb,
                        1.0,
                        model.h.get_untracked(),
                    ) {
                        model.select(color);
                    }
                }
                Err(()) => model
                    .message
                    .set("Could not sample the desktop pixel".into()),
                Ok(None) => {}
            }
            refs.sampling.set(false);
        };
        match hwnd.and_then(|hwnd| native::Session::start(hwnd, finish)) {
            Some(session) => on_cleanup_local(move || drop(session)),
            None => finish(Err(())),
        }
    }
    #[cfg(not(windows))]
    {
        model
            .message
            .set("Desktop sampling requires Windows".into());
        refs.sampling.set(false);
    }
}

#[cfg(windows)]
mod native {
    use std::{cell::Cell, ptr::null_mut, rc::Rc};
    use windows_sys::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
            Graphics::{Dwm::DwmFlush, Gdi::*},
            UI::{
                HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
                Input::KeyboardAndMouse::VK_ESCAPE,
                Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
                WindowsAndMessaging::*,
            },
        },
        w,
    };

    type Finish = dyn Fn(Result<Option<u32>, ()>);
    pub(super) struct Session {
        owner: HWND,
        chip: HWND,
        state: Box<State>,
        mouse_hook: HHOOK,
    }
    struct State {
        chip: HWND,
        pixel: Cell<Option<u32>>,
        scale: Cell<u32>,
        font: Cell<HFONT>,
        font_resource: *mut std::ffi::c_void,
        pressed: Cell<Option<u32>>,
        finished: Cell<bool>,
        finish: Rc<Finish>,
    }

    impl Session {
        pub(super) fn start(
            owner: HWND,
            finish: impl Fn(Result<Option<u32>, ()>) + 'static,
        ) -> Option<Self> {
            if !PREVIEW.get().is_null() {
                return None;
            }
            // Both HWNDs and their subclass callbacks are confined to the owning UI thread.
            unsafe {
                let chip = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    w!("STATIC"),
                    w!("OkColorPicker eyedropper"),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    owner,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                );
                if chip.is_null() {
                    return None;
                }
                let bytes = include_bytes!("../assets/MartianMono.ttf");
                let mut count = 0;
                let font_resource = AddFontMemResourceEx(
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    null_mut(),
                    &raw mut count,
                );
                let state = Box::new(State {
                    chip,
                    pixel: Cell::new(None),
                    scale: Cell::new(0),
                    font: Cell::new(null_mut()),
                    font_resource,
                    pressed: Cell::new(None),
                    finished: Cell::new(false),
                    finish: Rc::new(finish),
                });
                let ptr = (&*state as *const State) as usize;
                let mut session = Self {
                    owner,
                    chip,
                    state,
                    mouse_hook: null_mut(),
                };
                if SetWindowSubclass(chip, Some(chip_proc), 1, ptr) == 0
                    || SetWindowSubclass(owner, Some(owner_proc), 1, ptr) == 0
                {
                    return None;
                }
                let module = GetWindowLongPtrW(owner, GWLP_HINSTANCE) as _;
                session.mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
                if session.mouse_hook.is_null() {
                    return None;
                }
                PREVIEW.set(chip);
                // Also refresh when the desktop color changes beneath a stationary pointer.
                if SetTimer(chip, 1, 80, None) == 0 {
                    return None;
                }
                session.state.refresh();
                Some(session)
            }
        }
    }
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                RemoveWindowSubclass(self.owner, Some(owner_proc), 1);
                RemoveWindowSubclass(self.chip, Some(chip_proc), 1);
                if !self.mouse_hook.is_null() {
                    PREVIEW.set(null_mut());
                    MOVE_PENDING.set(false);
                    UnhookWindowsHookEx(self.mouse_hook);
                }
                DestroyWindow(self.chip);
                if !self.state.font.get().is_null() {
                    DeleteObject(self.state.font.get());
                }
                if !self.state.font_resource.is_null() {
                    RemoveFontMemResourceEx(self.state.font_resource);
                }
            }
        }
    }

    impl State {
        fn finish(&self, value: Result<Option<u32>, ()>) {
            if !self.finished.replace(true) {
                // Completing the reactive view can dispose this session during the callback.
                let finish = Rc::clone(&self.finish);
                finish(value);
            }
        }
        fn refresh(&self) {
            unsafe {
                let mut p = POINT::default();
                if GetCursorPos(&mut p) == 0 {
                    self.pixel.set(None);
                    return;
                }
                self.sample(p);
            }
        }
        fn sample_message(&self, owner: HWND, l: LPARAM) {
            let mut point = POINT {
                x: l as i16 as i32,
                y: (l >> 16) as i16 as i32,
            };
            if unsafe { ClientToScreen(owner, &mut point) } != 0 {
                self.sample(point);
            }
        }
        fn sample(&self, p: POINT) {
            unsafe {
                let monitor = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
                let mut info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetMonitorInfoW(monitor, &mut info) == 0 {
                    self.pixel.set(None);
                    return;
                }
                let mut dpi = 96;
                let mut dpi_y = 96;
                if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) < 0 {
                    dpi = 96;
                }
                let px = |n| scaled(n, dpi);
                let screen = info.rcMonitor;
                let x = chip_axis(p.x, px(5), px(82), screen.left, screen.right);
                let y = chip_axis(p.y, px(10), px(32), screen.top, screen.bottom);
                let mut old = RECT::default();
                GetWindowRect(self.chip, &mut old);
                let occluded = IsWindowVisible(self.chip) != 0
                    && p.x >= old.left
                    && p.x < old.right
                    && p.y >= old.top
                    && p.y < old.bottom;
                if occluded {
                    ShowWindow(self.chip, SW_HIDE);
                    DwmFlush();
                }
                let dc = GetDC(null_mut());
                let sampled = if dc.is_null() {
                    None
                } else {
                    let color = GetPixel(dc, p.x, p.y);
                    ReleaseDC(null_mut(), dc);
                    (color != u32::MAX).then_some(color)
                };
                let changed = self.pixel.replace(sampled) != sampled;
                let resized = self.scale.replace(dpi) != dpi;
                if resized {
                    let font = CreateFontW(
                        -px(10),
                        0,
                        0,
                        0,
                        FW_NORMAL as i32,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET as u32,
                        0,
                        0,
                        CLEARTYPE_QUALITY as u32,
                        0,
                        w!("Martian Mono"),
                    );
                    let old = self.font.replace(font);
                    if !old.is_null() {
                        DeleteObject(old);
                    }
                }
                if occluded
                    || old.left != x
                    || old.top != y
                    || resized
                    || IsWindowVisible(self.chip) == 0
                {
                    SetWindowPos(
                        self.chip,
                        HWND_TOPMOST,
                        x,
                        y,
                        px(82),
                        px(32),
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
                if changed || resized {
                    InvalidateRect(self.chip, std::ptr::null(), 0);
                }
            }
        }
        fn paint(&self) {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(self.chip, &mut ps);
                let px = |n| scaled(n, self.scale.get());
                let rect = |a, b, c, d| RECT {
                    left: px(a),
                    top: px(b),
                    right: px(c),
                    bottom: px(d),
                };
                let fill = |r: RECT, color| {
                    SetDCBrushColor(dc, color);
                    FillRect(dc, &r, GetStockObject(DC_BRUSH));
                };
                fill(rect(0, 0, 82, 32), 0x424242);
                fill(rect(1, 1, 81, 31), 0);
                if let Some(color) = self.pixel.get() {
                    fill(rect(5, 5, 27, 27), color);
                    let text = format!(
                        "{:02x}{:02x}{:02x}",
                        color as u8,
                        (color >> 8) as u8,
                        (color >> 16) as u8
                    );
                    let utf16: Vec<_> = text.encode_utf16().collect();
                    let font = SelectObject(dc, self.font.get());
                    SetTextColor(dc, 0xe0e0e0);
                    SetBkMode(dc, TRANSPARENT as i32);
                    DrawTextW(
                        dc,
                        utf16.as_ptr(),
                        6,
                        &mut rect(32, 1, 81, 31),
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                    );
                    SelectObject(dc, font);
                }
                EndPaint(self.chip, &ps);
            }
        }
    }

    fn scaled(n: i32, dpi: u32) -> i32 {
        (n * dpi as i32 + 48) / 96
    }
    fn chip_axis(pointer: i32, offset: i32, size: i32, min: i32, max: i32) -> i32 {
        let preferred = if pointer + offset + size <= max {
            pointer + offset
        } else {
            pointer - offset - size
        };
        preferred.clamp(min, (max - size).max(min))
    }

    const POINTER_MOVED: u32 = WM_APP + 1;
    const PICK_DOWN: u32 = WM_APP + 2;
    const PICK_UP: u32 = WM_APP + 3;
    const CANCEL_DOWN: u32 = WM_APP + 4;
    const CANCEL_UP: u32 = WM_APP + 5;
    thread_local! {
        static PREVIEW: Cell<HWND> = const { Cell::new(null_mut()) };
        static MOVE_PENDING: Cell<bool> = const { Cell::new(false) };
    }

    // Windows calls a low-level mouse hook on its installing thread. Only queue
    // the event here; all screen reads, painting and model edits run in the window.
    unsafe extern "system" fn mouse_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        let hwnd = PREVIEW.get();
        if code == HC_ACTION as i32 && !hwnd.is_null() {
            let message = match w as u32 {
                WM_LBUTTONDOWN => Some(PICK_DOWN),
                WM_LBUTTONUP => Some(PICK_UP),
                WM_RBUTTONDOWN => Some(CANCEL_DOWN),
                WM_RBUTTONUP => Some(CANCEL_UP),
                WM_MOUSEMOVE => {
                    if !MOVE_PENDING.replace(true)
                        && unsafe { PostMessageW(hwnd, POINTER_MOVED, 0, 0) } == 0
                    {
                        MOVE_PENDING.set(false);
                    }
                    None
                }
                _ => None,
            };
            if let Some(message) = message {
                let point = unsafe { (*(l as *const MSLLHOOKSTRUCT)).pt };
                unsafe {
                    PostMessageW(hwnd, message, point.x as usize, point.y as isize);
                }
                return 1;
            }
        }
        unsafe { CallNextHookEx(null_mut(), code, w, l) }
    }

    unsafe extern "system" fn chip_proc(
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        let state = unsafe { &*(data as *const State) };
        match msg {
            POINTER_MOVED => {
                MOVE_PENDING.set(false);
                state.refresh();
                0
            }
            PICK_DOWN | CANCEL_DOWN => {
                state.pressed.set(Some(msg));
                0
            }
            PICK_UP if state.pressed.take() == Some(PICK_DOWN) => {
                state.sample(POINT {
                    x: w as i32,
                    y: l as i32,
                });
                state.finish(state.pixel.get().map(Some).ok_or(()));
                0
            }
            CANCEL_UP if state.pressed.take() == Some(CANCEL_DOWN) => {
                state.finish(Ok(None));
                0
            }
            WM_PAINT => {
                state.paint();
                0
            }
            WM_ERASEBKGND => 1,
            WM_TIMER if w == 1 => {
                state.refresh();
                0
            }
            WM_NCHITTEST => HTTRANSPARENT as LRESULT,
            _ => unsafe { DefSubclassProc(hwnd, msg, w, l) },
        }
    }
    unsafe extern "system" fn owner_proc(
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        let state = unsafe { &*(data as *const State) };
        match msg {
            WM_MOUSEMOVE => {
                state.sample_message(hwnd, l);
                0
            }
            WM_KEYDOWN if w == VK_ESCAPE as usize => {
                state.finish(Ok(None));
                0
            }
            WM_CANCELMODE | WM_KILLFOCUS => {
                state.finish(Ok(None));
                unsafe { DefSubclassProc(hwnd, msg, w, l) }
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL | WM_CONTEXTMENU => 0,
            _ => unsafe { DefSubclassProc(hwnd, msg, w, l) },
        }
    }
    #[test]
    #[ignore = "requires the Windows desktop; exercises only a private hook callback"]
    fn desktop_pick_swallows_both_button_messages() {
        unsafe {
            let owner = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Test sampling owner"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            );
            assert!(!owner.is_null());
            for (down, up, queued_down, queued_up, cancel) in [
                (WM_LBUTTONDOWN, WM_LBUTTONUP, PICK_DOWN, PICK_UP, false),
                (WM_RBUTTONDOWN, WM_RBUTTONUP, CANCEL_DOWN, CANCEL_UP, true),
            ] {
                let completed = Rc::new(Cell::new(false));
                let finished = completed.clone();
                let session = Session::start(owner, move |result| {
                    assert_eq!(matches!(result, Ok(None)), cancel);
                    finished.set(true);
                })
                .expect("temporary desktop sampler");
                let event = MSLLHOOKSTRUCT {
                    pt: POINT { x: -20, y: 10 },
                    ..Default::default()
                };
                for (button, expected) in [(down, queued_down), (up, queued_up)] {
                    assert_eq!(
                        mouse_proc(
                            HC_ACTION as i32,
                            button as usize,
                            &raw const event as LPARAM
                        ),
                        1,
                        "the event must stop before another application's input queue"
                    );
                    let mut message = MSG::default();
                    assert_ne!(
                        PeekMessageW(&mut message, session.chip, expected, expected, PM_REMOVE),
                        0
                    );
                    assert_eq!((message.wParam as i32, message.lParam as i32), (-20, 10));
                    DispatchMessageW(&message);
                    assert_eq!(completed.get(), button == up);
                }
                drop(session);
                assert!(PREVIEW.get().is_null());
            }
            DestroyWindow(owner);
        }
    }
}
