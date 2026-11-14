use super::*;
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;

type Messages = RefCell<Vec<(u32, LPARAM)>>;
struct TestWindow {
    hwnd: HWND,
    messages: Box<Messages>,
}

impl TestWindow {
    fn new() -> Self {
        // SAFETY: create only this test's hidden HWND; no external UI is inspected.
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP,
                windows_sys::w!("STATIC"),
                windows_sys::w!("Caption test"),
                WS_OVERLAPPEDWINDOW,
                -800,
                -600,
                330,
                560,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null(), "{}", io::Error::last_os_error());
            let messages = Box::new(RefCell::new(Vec::new()));
            assert_ne!(
                SetWindowSubclass(
                    hwnd,
                    Some(observe),
                    2,
                    messages.as_ref() as *const Messages as usize
                ),
                0
            );
            Self { hwnd, messages }
        }
    }
    fn screen_point(&self, x: i32, y: i32) -> LPARAM {
        let mut point = POINT { x, y };
        // SAFETY: this test retains its HWND and output POINT for the call.
        assert_ne!(unsafe { ClientToScreen(self.hwnd, &mut point) }, 0);
        pack_point(point)
    }
    fn send(&self, message: u32, wparam: usize, lparam: LPARAM) -> LRESULT {
        // SAFETY: synchronous message to the owned test window on its UI thread.
        unsafe { SendMessageW(self.hwnd, message, wparam, lparam) }
    }
    fn events(&self) -> Vec<(u32, LPARAM)> {
        std::mem::take(&mut *self.messages.borrow_mut())
    }
}
impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: detach before freeing the observation data, then destroy our HWND.
        unsafe {
            RemoveWindowSubclass(self.hwnd, Some(observe), 2);
            DestroyWindow(self.hwnd);
        }
    }
}
unsafe extern "system" fn observe(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: TestWindow owns the boxed log through callback removal.
    unsafe {
        if msg == WM_NCHITTEST {
            return HTCLIENT as LRESULT;
        }
        if matches!(
            msg,
            WM_MOUSEMOVE
                | WM_MOUSELEAVE
                | WM_NCMOUSEMOVE
                | WM_NCMOUSELEAVE
                | WM_LBUTTONDOWN
                | WM_LBUTTONUP
                | WM_NCLBUTTONDOWN
                | WM_NCLBUTTONUP
                | WM_CANCELMODE
        ) {
            (&*(data as *const Messages))
                .borrow_mut()
                .push((msg, lparam));
            return 0;
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

#[test]
fn native_hit_test_uses_actual_layout_negative_screen_coordinates_and_invalidates_stale_layout() {
    let window = TestWindow::new();
    // SAFETY: window outlives the hook on this thread.
    let hook = unsafe { Hook::install(window.hwnd) }.unwrap();
    let center = window.screen_point(30, 20);
    assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
    hook.update(Some([20.0, 10.0, 55.0, 40.0]));
    for (x, y, expected) in [
        (20, 10, HTMAXBUTTON),
        (54, 39, HTMAXBUTTON),
        (19, 20, HTCLIENT),
        (55, 20, HTCLIENT),
        (30, 9, HTCLIENT),
        (30, 40, HTCLIENT),
    ] {
        assert_eq!(
            window.send(WM_NCHITTEST, 0, window.screen_point(x, y)),
            expected as isize
        );
    }
    for message in [WM_SIZE, WM_DPICHANGED] {
        // WM_DPICHANGED needs a real RECT because the default window procedure may read it.
        let suggested = RECT {
            left: -800,
            top: -600,
            right: -470,
            bottom: -40,
        };
        window.send(message, 0, &suggested as *const RECT as LPARAM);
        assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
        hook.update(Some([20.0, 10.0, 55.0, 40.0]));
    }
    // Layout changes (including a new DPI) supply new physical pixel bounds.
    hook.update(Some([40.0, 20.0, 110.0, 80.0]));
    assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
    assert_eq!(
        window.send(WM_NCHITTEST, 0, window.screen_point(80, 50)),
        HTMAXBUTTON as isize
    );
    // Fractional DPI must not round an outside pixel into the button.
    hook.update(Some([20.25, 10.25, 55.25, 40.25]));
    for (x, y, expected) in [
        (20, 20, HTCLIENT),
        (21, 11, HTMAXBUTTON),
        (55, 40, HTMAXBUTTON),
        (56, 40, HTCLIENT),
    ] {
        assert_eq!(
            window.send(WM_NCHITTEST, 0, window.screen_point(x, y)),
            expected as isize
        );
    }
    hook.update(Some([f32::NAN, 0.0, 50.0, 20.0]));
    assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
    hook.update(Some([20.0, 10.0, 55.0, 40.0]));
    drop(hook);
    assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
}

#[test]
fn native_caption_hover_and_click_forward_once_without_a_second_native_click() {
    let window = TestWindow::new();
    // SAFETY: window outlives the hook on this thread.
    let hook = unsafe { Hook::install(window.hwnd) }.unwrap();
    hook.update(Some([20.0, 10.0, 55.0, 40.0]));
    window.events();
    let screen = window.screen_point(30, 20);
    let client = pack_point(POINT { x: 30, y: 20 });
    window.send(WM_NCMOUSEMOVE, HTMAXBUTTON as usize, screen);
    assert_eq!(
        window.events(),
        [(WM_MOUSEMOVE, client), (WM_NCMOUSEMOVE, screen)]
    );
    window.send(WM_NCLBUTTONDOWN, HTMAXBUTTON as usize, screen);
    window.send(WM_NCLBUTTONUP, HTMAXBUTTON as usize, screen);
    assert_eq!(
        window.events(),
        [
            (WM_MOUSEMOVE, client),
            (WM_LBUTTONDOWN, client),
            (WM_MOUSEMOVE, client),
            (WM_LBUTTONUP, client)
        ]
    );
    window.send(WM_NCMOUSELEAVE, 0, 0);
    assert_eq!(window.events(), [(WM_MOUSELEAVE, 0), (WM_NCMOUSELEAVE, 0)]);
    window.send(WM_NCMOUSEMOVE, HTMAXBUTTON as usize, screen);
    window.events();
    window.send(WM_MOUSEMOVE, 0, client);
    window.send(WM_NCMOUSELEAVE, 0, 0);
    assert_eq!(
        window.events(),
        [(WM_MOUSEMOVE, client), (WM_NCMOUSELEAVE, 0)]
    );
    window.send(WM_NCLBUTTONDOWN, HTMAXBUTTON as usize, screen);
    window.send(WM_CANCELMODE, 0, 0);
    window.events();
    window.send(WM_NCLBUTTONUP, HTMAXBUTTON as usize, screen);
    assert!(!window.events().iter().any(|(msg, _)| *msg == WM_LBUTTONUP));
}
