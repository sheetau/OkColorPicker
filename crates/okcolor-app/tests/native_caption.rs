//! End-to-end native messages within this test's own Picker window; no desktop input injection.
#![cfg(target_os = "windows")]
use okcolor_app::{
    fonts, graphics,
    model::PickerModel,
    ui::{PickerProps, PickerRefs, STYLE},
    windows_caption::CaptionBridge,
};
use raw_window_handle::RawWindowHandle;
use std::{cell::Cell, rc::Rc, time::Duration};
use windows_sys::Win32::{
    Foundation::POINT,
    Graphics::Gdi::ClientToScreen,
    UI::{Input::KeyboardAndMouse::GetCapture, WindowsAndMessaging::*},
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
use zgui::{
    prelude::*,
    runtime::{FrameProbe, Window},
};
use zgui_platform_winit::{UserEvent, WinitApp};

struct Probe {
    caption: CaptionBridge,
    button: Rc<Cell<Option<NodeRef>>>,
    stage: Cell<u8>,
    frames: Cell<u32>,
}

impl FrameProbe for Probe {
    fn frame_ended(&self, window: &Window) {
        self.caption.frame_ended(window);
        let frames = self.frames.get() + 1;
        self.frames.set(frames);
        assert!(frames < 60, "Native caption round trip did not settle");
        let maximized = window.surface().is_maximized();
        match (self.stage.get(), maximized) {
            (0 | 1, false) | (2, true) => {
                let cancel = self.stage.get() == 0;
                let bounds = self.button.get().unwrap().window_bounds().unwrap();
                let gpu = window.surface().clone().gpu_shared().unwrap();
                let handle = gpu.window_handle().unwrap();
                let RawWindowHandle::Win32(native) = handle.as_raw() else {
                    panic!("Expected HWND")
                };
                let hwnd = native.hwnd.get() as _;
                let point = POINT {
                    x: (bounds.origin.x.0 + bounds.size.width.0 / 2.0).round() as i32,
                    y: (bounds.origin.y.0 + bounds.size.height.0 / 2.0).round() as i32,
                };
                let mut screen = point;
                // SAFETY: the renderer's surface retains this test-owned HWND.
                // Send only to this HWND; no global mouse or keyboard input is generated.
                unsafe {
                    assert_ne!(ClientToScreen(hwnd, &mut screen), 0);
                    let pack =
                        |p: POINT| (p.x as u16 as u32 | ((p.y as u16 as u32) << 16)) as isize;
                    let hit = SendMessageW(hwnd, WM_NCHITTEST, 0, pack(screen));
                    assert_eq!(
                        hit, HTMAXBUTTON as isize,
                        "Painted button must be a Snap target"
                    );
                    self.stage.set(self.stage.get() + 1);
                    SendMessageW(hwnd, WM_NCMOUSEMOVE, HTMAXBUTTON as usize, pack(screen));
                    SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTMAXBUTTON as usize, pack(screen));
                    assert_eq!(
                        GetCapture(),
                        hwnd,
                        "Winit must capture the translated press"
                    );
                    // Releasing outside the button must cancel the first gesture.
                    let release = if cancel {
                        POINT { x: 20, y: 80 }
                    } else {
                        point
                    };
                    SendMessageW(hwnd, WM_MOUSEMOVE, 0, pack(release));
                    // Captured mouse-up is a client message in the actual Windows path.
                    SendMessageW(hwnd, WM_LBUTTONUP, 0, pack(release));
                    assert_ne!(GetCapture(), hwnd);
                }
            }
            (3, false) => {
                self.stage.set(4);
                window.handle().close();
            }
            _ => {}
        }
        window.request_frame();
    }
}

#[test]
#[ignore = "requires a Windows desktop and DX12 GPU; opens its own temporary window"]
fn painted_native_maximize_button_round_trips_through_winit_and_zgui() {
    let (finish, wait) = std::sync::mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if matches!(
            wait.recv_timeout(Duration::from_secs(30)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ) {
            eprintln!("Native caption test timed out");
            std::process::exit(1);
        }
    });
    let caption = CaptionBridge::new(false);
    let button = Rc::new(Cell::new(None));
    let probe = Rc::new(Probe {
        caption: caption.clone(),
        button: button.clone(),
        stage: Cell::new(0),
        frames: Cell::new(0),
    });
    app()
        .with_title("OkColorPicker native caption test")
        .with_size(330.0, 560.0)
        .with_min_size(330.0, 520.0)
        .with_decorations(Decorations::NoTitleBar)
        .with_transparent(true)
        .with_fonts(fonts::load())
        .with_stylesheet(STYLE)
        .with_renderer(caption.wrap_renderer(graphics::factory()))
        .with_probe(probe.clone())
        .run_on(
            |handler| {
                let event_loop = EventLoop::<UserEvent>::with_user_event()
                    .with_any_thread(true)
                    .build()
                    .unwrap();
                let mut native = WinitApp::new(&event_loop, handler);
                event_loop
                    .run_app(&mut native)
                    .map_err(|e| zgui::platform::PlatformError::Backend(e.to_string()))
            },
            move || {
                let refs = PickerRefs::default();
                caption.bind_maximize(refs.maximize);
                button.set(Some(refs.maximize));
                view! { Picker(model=PickerModel::default(),refs=refs) }
            },
        )
        .unwrap();
    assert_eq!(
        probe.stage.get(),
        4,
        "Cancellation, maximize and restore must finish"
    );
    finish.send(()).unwrap();
    watchdog.join().unwrap();
}
