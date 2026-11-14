use super::*;
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;

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
    hook.update([None, Some([20.0, 10.0, 55.0, 40.0]), None]);
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
        hook.update([None, Some([20.0, 10.0, 55.0, 40.0]), None]);
    }
    // Layout changes (including a new DPI) supply new physical pixel bounds.
    hook.update([None, Some([40.0, 20.0, 110.0, 80.0]), None]);
    assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
    assert_eq!(
        window.send(WM_NCHITTEST, 0, window.screen_point(80, 50)),
        HTMAXBUTTON as isize
    );
    // Fractional DPI must not round an outside pixel into the button.
    hook.update([None, Some([20.25, 10.25, 55.25, 40.25]), None]);
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
    hook.update([None, Some([f32::NAN, 0.0, 50.0, 20.0]), None]);
    assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
    hook.update([None, Some([20.0, 10.0, 55.0, 40.0]), None]);
    drop(hook);
    assert_eq!(window.send(WM_NCHITTEST, 0, center), HTCLIENT as isize);
}

#[test]
fn native_caption_owns_three_gestures_and_queues_one_system_command() {
    let window = TestWindow::new();
    // SAFETY: window outlives the hook on this thread.
    let hook = unsafe { Hook::install(window.hwnd) }.unwrap();
    hook.update([
        Some([0.0, 10.0, 36.0, 41.0]),
        Some([36.0, 10.0, 72.0, 41.0]),
        Some([72.0, 10.0, 108.0, 41.0]),
    ]);
    let commands = || {
        let mut commands = vec![];
        let mut message = MSG::default();
        // SAFETY: consume only queued system commands belonging to our hidden test HWND.
        unsafe {
            while PeekMessageW(
                &mut message,
                window.hwnd,
                WM_SYSCOMMAND,
                WM_SYSCOMMAND,
                PM_REMOVE,
            ) != 0
            {
                commands.push(message.wParam as u32);
            }
        }
        commands
    };
    for (index, (hit_code, command)) in [
        (HTMINBUTTON, SC_MINIMIZE),
        (HTMAXBUTTON, SC_MAXIMIZE),
        (HTCLOSE, SC_CLOSE),
    ]
    .into_iter()
    .enumerate()
    {
        let screen = window.screen_point(index as i32 * 36 + 18, 20);
        assert_eq!(window.send(WM_NCHITTEST, 0, screen), hit_code as isize);
        window.events();
        window.send(WM_NCMOUSEMOVE, hit_code as usize, screen);
        assert_eq!(hook.state.hovered.get(), Some(index));
        assert_eq!(
            window.events(),
            [(WM_NCMOUSEMOVE, screen)],
            "no synthetic client hover"
        );
        window.send(WM_NCLBUTTONDOWN, hit_code as usize, screen);
        // SAFETY: scalar thread-local capture query.
        assert_eq!(unsafe { GetCapture() }, window.hwnd);
        assert_eq!(hook.state.pressed.get(), Some(index));
        window.send(WM_MOUSEMOVE, 0, pack_point(POINT { x: 120, y: 80 }));
        assert_eq!(hook.state.hovered.get(), None);
        window.send(WM_LBUTTONUP, 0, pack_point(POINT { x: 120, y: 80 }));
        assert!(commands().is_empty(), "outside release must cancel");
        assert_eq!(hook.state.pressed.get(), None);
        window.send(WM_NCLBUTTONDOWN, hit_code as usize, screen);
        window.send(WM_NCLBUTTONUP, hit_code as usize, screen);
        assert_eq!(commands(), [command]);
        assert!(
            !window
                .events()
                .iter()
                .any(|(msg, _)| matches!(*msg, WM_LBUTTONDOWN | WM_LBUTTONUP)),
            "zgui must not get a second click target"
        );
        window.send(WM_NCLBUTTONDOWN, hit_code as usize, screen);
        window.send(WM_CANCELMODE, 0, 0);
        window.send(WM_NCLBUTTONUP, hit_code as usize, screen);
        assert!(commands().is_empty());
    }
    // SAFETY: alter only our hidden test HWND's capabilities.
    unsafe {
        let style = GetWindowLongPtrW(window.hwnd, GWL_STYLE);
        SetWindowLongPtrW(
            window.hwnd,
            GWL_STYLE,
            style & !(WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize,
        );
    }
    assert_eq!(
        window.send(WM_NCHITTEST, 0, window.screen_point(18, 20)),
        HTCLIENT as isize
    );
    assert_eq!(
        window.send(WM_NCHITTEST, 0, window.screen_point(54, 20)),
        HTCLIENT as isize
    );
    assert_eq!(
        window.send(WM_NCHITTEST, 0, window.screen_point(90, 20)),
        HTCLOSE as isize
    );
}
