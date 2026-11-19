//! A transient desktop eyedropper. A Windows mouse hook consumes the sampling
//! click through button-up; an independent preview follows the cursor across displays.
use crate::{model::PickerModel, ui::PickerRefs};
use zgui::prelude::*;

#[cfg(windows)]
type SampleResult = Result<Option<u32>, &'static str>;

#[component]
pub(crate) fn Eyedropper(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    #[cfg(windows)]
    if let Some(owner) =
        use_context::<StoredValue<crate::windows_caption::CaptionBridge, LocalStorage>>()
    {
        let hwnd = owner.with_value(|bridge| bridge.hwnd());
        let finish = move |result: SampleResult| {
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
                Err(message) => model.message.set(message.into()),
                Ok(None) => {}
            }
            refs.sampling.set(false);
        };
        match hwnd.and_then(|hwnd| native::Session::start(hwnd, finish)) {
            Some(session) => on_cleanup_local(move || drop(session)),
            None => finish(Err("Could not start the desktop eyedropper")),
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
    use super::SampleResult;
    use crate::frozen_desktop::FrozenDesktop;
    use ab_glyph::{Font, FontRef, ScaleFont, point};
    use std::{
        cell::{Cell, RefCell},
        ptr::null_mut,
        rc::Rc,
    };
    use windows_sys::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
            Graphics::{Dwm::DwmFlush, Gdi::*},
            UI::{
                HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
                Input::KeyboardAndMouse::{VK_ESCAPE, VK_LMENU, VK_RMENU},
                Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
                WindowsAndMessaging::*,
            },
        },
        w,
    };

    type Finish = dyn Fn(SampleResult);
    pub(super) struct Session {
        owner: HWND,
        chip: HWND,
        state: Rc<State>,
        mouse_hook: HHOOK,
        keyboard_hook: HHOOK,
    }
    struct State {
        chip: HWND,
        frozen: RefCell<Option<FrozenDesktop>>,
        alt_keys: Cell<u8>,
        pixel: Cell<Option<u32>>,
        scale: Cell<u32>,
        font: FontRef<'static>,
        pressed: Cell<Option<u32>>,
        finished: Cell<bool>,
        finish: Rc<Finish>,
    }

    impl Session {
        pub(super) fn start(owner: HWND, finish: impl Fn(SampleResult) + 'static) -> Option<Self> {
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
                let state = Rc::new(State {
                    chip,
                    frozen: RefCell::new(None),
                    alt_keys: Cell::new(0),
                    pixel: Cell::new(None),
                    scale: Cell::new(0),
                    font: FontRef::try_from_slice(include_bytes!("../assets/MartianMono.ttf"))
                        .expect("embedded Martian Mono"),
                    pressed: Cell::new(None),
                    finished: Cell::new(false),
                    finish: Rc::new(finish),
                });
                let ptr = Rc::as_ptr(&state) as usize;
                let mut session = Self {
                    owner,
                    chip,
                    state,
                    mouse_hook: null_mut(),
                    keyboard_hook: null_mut(),
                };
                if SetWindowSubclass(chip, Some(chip_proc), 1, ptr) == 0
                    || SetWindowSubclass(owner, Some(owner_proc), 1, ptr) == 0
                {
                    return None;
                }
                let module = GetWindowLongPtrW(owner, GWLP_HINSTANCE) as _;
                session.mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
                session.keyboard_hook =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0);
                if session.mouse_hook.is_null() || session.keyboard_hook.is_null() {
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
                self.state.finished.set(true);
                RemoveWindowSubclass(self.owner, Some(owner_proc), 1);
                RemoveWindowSubclass(self.chip, Some(chip_proc), 1);
                if !self.keyboard_hook.is_null() {
                    UnhookWindowsHookEx(self.keyboard_hook);
                }
                self.state.frozen.take();
                if !self.mouse_hook.is_null() {
                    PREVIEW.set(null_mut());
                    MOVE_PENDING.set(false);
                    FREEZE_PENDING.set(false);
                    UnhookWindowsHookEx(self.mouse_hook);
                }
                DestroyWindow(self.chip);
            }
        }
    }

    impl State {
        fn finish(&self, value: SampleResult) {
            if !self.finished.replace(true) {
                // Completing the reactive view can dispose this session during the callback.
                let finish = Rc::clone(&self.finish);
                finish(value);
            }
        }
        fn alt(&self, mask: u8, down: bool) {
            let previous = self.alt_keys.get();
            let next = if down {
                previous | mask
            } else {
                previous & !mask
            };
            self.alt_keys.set(next);
            if previous == 0 && next != 0 {
                unsafe {
                    ShowWindow(self.chip, SW_HIDE);
                    DwmFlush();
                }
                let Some(desktop) =
                    FrozenDesktop::capture(unsafe { GetWindow(self.chip, GW_OWNER) })
                else {
                    self.finish(Err("Could not freeze the current desktop"));
                    return;
                };
                if self.finished.get() || self.alt_keys.get() == 0 {
                    return;
                }
                self.frozen.replace(Some(desktop));
                self.refresh();
            } else if next == 0 && previous != 0 {
                self.frozen.take();
                unsafe {
                    DwmFlush();
                }
                self.refresh();
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
                let x = chip_axis(p.x, px(5), px(5), px(82), screen.left, screen.right);
                let y = chip_axis(p.y, px(10), 0, px(32), screen.top, screen.bottom);
                let mut old = RECT::default();
                GetWindowRect(self.chip, &mut old);
                let frozen = self.frozen.borrow();
                let occluded = frozen.is_none()
                    && IsWindowVisible(self.chip) != 0
                    && p.x >= old.left
                    && p.x < old.right
                    && p.y >= old.top
                    && p.y < old.bottom;
                if occluded {
                    ShowWindow(self.chip, SW_HIDE);
                    DwmFlush();
                }
                let sampled = if let Some(desktop) = frozen.as_ref() {
                    desktop.pixel(p)
                } else {
                    let dc = GetDC(null_mut());
                    if dc.is_null() {
                        None
                    } else {
                        let color = GetPixel(dc, p.x, p.y);
                        ReleaseDC(null_mut(), dc);
                        (color != CLR_INVALID).then_some(color)
                    }
                };
                drop(frozen);
                let changed = self.pixel.replace(sampled) != sampled;
                let resized = self.scale.replace(dpi) != dpi;
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
                    let em = 10.0 * self.scale.get() as f32 / 96.0;
                    let font = self.font.as_scaled(
                        em * self.font.height_unscaled()
                            / self.font.units_per_em().expect("OpenType em"),
                    );
                    let baseline = px(16) as f32 + (font.ascent() + font.descent()) * 0.5;
                    let mut x = px(32) as f32;
                    for ch in text.chars() {
                        let id = font.glyph_id(ch);
                        let glyph = id.with_scale_and_position(font.scale(), point(x, baseline));
                        if let Some(outline) = self.font.outline_glyph(glyph) {
                            let bounds = outline.px_bounds();
                            outline.draw(|dx, dy, coverage| {
                                let gray = (224.0 * coverage).round() as u32;
                                SetPixelV(
                                    dc,
                                    bounds.min.x as i32 + dx as i32,
                                    bounds.min.y as i32 + dy as i32,
                                    gray * 0x010101,
                                );
                            });
                        }
                        x += font.h_advance(id);
                    }
                }
                EndPaint(self.chip, &ps);
            }
        }
    }

    fn scaled(n: i32, dpi: u32) -> i32 {
        (n * dpi as i32 + 48) / 96
    }
    fn chip_axis(
        pointer: i32,
        offset: i32,
        reverse_offset: i32,
        size: i32,
        min: i32,
        max: i32,
    ) -> i32 {
        let preferred = if pointer + offset + size <= max {
            pointer + offset
        } else {
            pointer - reverse_offset - size
        };
        preferred.clamp(min, (max - size).max(min))
    }

    const ALT_CHANGED: u32 = WM_APP + 6;
    const POINTER_MOVED: u32 = WM_APP + 1;
    const PICK_DOWN: u32 = WM_APP + 2;
    const PICK_UP: u32 = WM_APP + 3;
    const CANCEL_DOWN: u32 = WM_APP + 4;
    const CANCEL_UP: u32 = WM_APP + 5;
    thread_local! {
        static PREVIEW: Cell<HWND> = const { Cell::new(null_mut()) };
        static FREEZE_PENDING: Cell<bool> = const { Cell::new(false) };
        static MOVE_PENDING: Cell<bool> = const { Cell::new(false) };
    }

    unsafe extern "system" fn keyboard_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        let hwnd = PREVIEW.get();
        if code == HC_ACTION as i32 && !hwnd.is_null() {
            let key = unsafe { (*(l as *const KBDLLHOOKSTRUCT)).vkCode };
            let mask = if key == u32::from(VK_LMENU) {
                1
            } else if key == u32::from(VK_RMENU) {
                2
            } else {
                0
            };
            if mask != 0 {
                let down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
                if down {
                    FREEZE_PENDING.set(true);
                }
                if unsafe { PostMessageW(hwnd, ALT_CHANGED, mask, isize::from(down)) } == 0 {
                    FREEZE_PENDING.set(false);
                }
                return 1;
            }
        }
        unsafe { CallNextHookEx(null_mut(), code, w, l) }
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
                WM_MOUSEMOVE if FREEZE_PENDING.get() => return 1,
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

    unsafe fn retain_state(data: usize) -> Rc<State> {
        let ptr = data as *const State;
        unsafe {
            Rc::increment_strong_count(ptr);
            Rc::from_raw(ptr)
        }
    }

    unsafe extern "system" fn chip_proc(
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        // Capture and window teardown can reenter the message loop. Retain the callback state.
        let state = unsafe { retain_state(data) };
        match msg {
            ALT_CHANGED => {
                state.alt(w as u8, l != 0);
                FREEZE_PENDING.set(false);
                0
            }
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
                state.finish(
                    state
                        .pixel
                        .get()
                        .map(Some)
                        .ok_or("Could not sample the desktop pixel"),
                );
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
        // Capture and window teardown can reenter the message loop. Retain the callback state.
        let state = unsafe { retain_state(data) };
        match msg {
            WM_MOUSEMOVE => {
                state.sample_message(hwnd, l);
                0
            }
            WM_KEYDOWN if w == VK_ESCAPE as usize => {
                state.finish(Ok(None));
                0
            }
            WM_DISPLAYCHANGE | WM_CANCELMODE | WM_KILLFOCUS => {
                state.finish(Ok(None));
                unsafe { DefSubclassProc(hwnd, msg, w, l) }
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL | WM_CONTEXTMENU => 0,
            _ => unsafe { DefSubclassProc(hwnd, msg, w, l) },
        }
    }
    #[test]
    #[ignore = "requires an interactive Windows desktop for comparing visible pixels"]
    fn alt_freezes_visible_pixels_and_releases_each_capture() {
        unsafe {
            let owner = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
                w!("BUTTON"),
                w!("Eyedropper test source"),
                WS_POPUP,
                140,
                140,
                64,
                64,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            );
            assert!(!owner.is_null());
            let fill = |color| {
                let dc = GetDC(owner);
                let brush = CreateSolidBrush(color);
                FillRect(
                    dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: 64,
                        bottom: 64,
                    },
                    brush,
                );
                DeleteObject(brush);
                ReleaseDC(owner, dc);
                GdiFlush();
                DwmFlush();
            };
            SetLayeredWindowAttributes(owner, 0, 255, LWA_ALPHA);
            ShowWindow(owner, SW_SHOWNOACTIVATE);
            UpdateWindow(owner);
            let picked = Rc::new(Cell::new(None));
            let finish = picked.clone();
            let session = Session::start(owner, move |result| finish.set(result.unwrap())).unwrap();
            let mut p = POINT { x: 27, y: 26 };
            ClientToScreen(owner, &mut p);
            let read = || {
                let dc = GetDC(null_mut());
                let rgb = GetPixel(dc, p.x, p.y);
                ReleaseDC(null_mut(), dc);
                rgb
            };
            let key = |vk, down| {
                let event = KBDLLHOOKSTRUCT {
                    vkCode: u32::from(vk),
                    ..Default::default()
                };
                let msg = if down { WM_SYSKEYDOWN } else { WM_SYSKEYUP };
                assert_eq!(
                    keyboard_proc(HC_ACTION as i32, msg as usize, &raw const event as isize),
                    1
                );
                let mut pending = MSG::default();
                assert_ne!(
                    PeekMessageW(
                        &mut pending,
                        session.chip,
                        ALT_CHANGED,
                        ALT_CHANGED,
                        PM_REMOVE
                    ),
                    0
                );
                DispatchMessageW(&pending);
                DwmFlush();
            };
            fill(0x42a1ef);
            let original = read();
            assert_eq!(
                original, 0x42a1ef,
                "fixture must be visible before freezing"
            );
            key(VK_LMENU, true);
            assert!(session.state.frozen.borrow().is_some());
            assert_eq!(
                read(),
                original,
                "the overlay must reproduce the desktop pixel"
            );
            let overlay = WindowFromPoint(p);
            assert_ne!(overlay, owner, "the frozen desktop must receive hover");
            key(VK_LMENU, true);
            key(VK_RMENU, true);
            fill(0xe17730);
            key(VK_LMENU, false);
            session.state.sample(p);
            assert_eq!(session.state.pixel.get(), Some(original));
            assert_eq!(read(), original, "background repaints must remain hidden");
            key(VK_RMENU, false);
            assert!(session.state.frozen.borrow().is_none());
            assert_eq!(IsWindow(overlay), 0);
            assert_eq!(WindowFromPoint(p), owner);
            let replacement = read();
            assert_ne!(replacement, original);
            key(VK_RMENU, true);
            fill(0x509266);
            SendMessageW(session.chip, PICK_DOWN, 0, 0);
            SendMessageW(session.chip, PICK_UP, p.x as usize, p.y as isize);
            assert_eq!(
                picked.get(),
                Some(replacement),
                "a second hold must capture the new frame"
            );
            let overlay = WindowFromPoint(p);
            drop(session);
            assert_eq!(IsWindow(overlay), 0);
            DestroyWindow(owner);
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
