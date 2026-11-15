//! Win32 messages addressed only to this test's own window; no global input or clipboard changes.
#![cfg(target_os = "windows")]
use okcolor_app::{
    fonts, graphics,
    model::PickerModel,
    ui::{PickerProps, PickerRefs, STYLE},
};
use raw_window_handle::RawWindowHandle;
use std::{cell::Cell, rc::Rc, time::Duration};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    Graphics::Gdi::ClientToScreen,
    UI::{
        Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC, MapVirtualKeyW, VK_UP},
        WindowsAndMessaging::*,
    },
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
use zgui::{
    prelude::*,
    runtime::{FrameProbe, Window},
};
use zgui_platform_winit::{UserEvent, WinitApp};

#[path = "support/observe_renderer.rs"]
mod observe_renderer;
use observe_renderer::ObservePresentation;

fn pack(point: POINT) -> LPARAM {
    (point.x as u16 as u32 | ((point.y as u16 as u32) << 16)) as LPARAM
}
fn position(node: NodeRef, x: f32, y: f32) -> POINT {
    let b = node.window_bounds().expect("mounted control");
    POINT {
        x: (b.origin.x.0 + b.size.width.0 * x).round() as i32,
        y: (b.origin.y.0 + b.size.height.0 * y).round() as i32,
    }
}
fn mouse(hwnd: HWND, message: u32, point: POINT) {
    // SAFETY: the probe retains this owned HWND for the synchronous call.
    unsafe {
        SendMessageW(
            hwnd,
            message,
            if message == WM_LBUTTONDOWN { 1 } else { 0 },
            pack(point),
        );
    }
}

struct Probe {
    state: Rc<Cell<Option<(PickerModel, PickerRefs)>>>,
    stage: Cell<u8>,
    frames: Cell<usize>,
    base: Cell<Option<okcolor_core::Oklch>>,
    recent: Cell<usize>,
    extent: Cell<[i32; 2]>,
    location: Cell<[i32; 2]>,
    presented: Rc<Cell<[i32; 2]>>,
}

impl FrameProbe for Probe {
    fn frame_ended(&self, window: &Window) {
        self.frames.set(self.frames.get() + 1);
        assert!(
            self.frames.get() < 120,
            "native input sequence stalled at {}",
            self.stage.get()
        );
        let (model, refs) = self.state.get().unwrap();
        let surface = window.surface().clone().gpu_shared().unwrap();
        let handle = surface.window_handle().unwrap();
        let RawWindowHandle::Win32(native) = handle.as_raw() else {
            panic!("Win32 required")
        };
        let hwnd = native.hwnd.get() as HWND;
        let mut client = RECT::default();
        let mut rect = RECT::default();
        // SAFETY: synchronous queries only on the test-owned HWND retained above.
        unsafe {
            assert_ne!(GetClientRect(hwnd, &mut client), 0);
            assert_ne!(GetWindowRect(hwnd, &mut rect), 0);
        }
        match self.stage.get() {
            0 => {
                self.base.set(Some(model.current()));
                self.recent.set(model.recent.get_untracked().len());
                self.extent.set([client.right, client.bottom]);
                self.stage.set(1);
                let point = position(refs.slice, 0.05, 0.5);
                mouse(hwnd, WM_MOUSEMOVE, point);
                mouse(hwnd, WM_LBUTTONDOWN, point);
            }
            1 if model.dragging.get_untracked() => {
                assert_ne!(model.current(), self.base.get().unwrap());
                assert_eq!(model.recent.get_untracked().len(), self.recent.get());
                self.base.set(Some(model.current()));
                self.stage.set(2);
                mouse(hwnd, WM_MOUSEMOVE, position(refs.slice, 1.4, 0.3));
            }
            2 if model.current() != self.base.get().unwrap() => {
                assert!(model.dragging.get_untracked());
                assert!(model.current().in_gamut(model.gamut.get_untracked()));
                self.stage.set(3);
                mouse(hwnd, WM_LBUTTONUP, position(refs.slice, 1.4, 0.3));
            }
            3 if !model.dragging.get_untracked() => {
                assert_eq!(model.recent.get_untracked().len(), self.recent.get() + 1);
                assert!(model.can_undo.get_untracked());
                self.stage.set(4);
                let point = position(refs.numbers[1], 0.5, 0.5);
                mouse(hwnd, WM_MOUSEMOVE, point);
                mouse(hwnd, WM_LBUTTONDOWN, point);
                mouse(hwnd, WM_LBUTTONUP, point);
            }
            4 if model.editing_text.get_untracked() => {
                self.base.set(Some(model.current()));
                self.recent.set(model.recent.get_untracked().len());
                self.stage.set(5);
                let mut point = position(refs.numbers[1], 0.5, 0.5);
                // SAFETY: the wheel message uses screen coordinates of our field, not desktop input.
                unsafe {
                    assert_ne!(ClientToScreen(hwnd, &mut point), 0);
                    SendMessageW(hwnd, WM_MOUSEWHEEL, (120_u32 << 16) as usize, pack(point));
                }
            }
            5 if model.current() != self.base.get().unwrap() => {
                assert!(
                    (model.current().components()[1]
                        - self.base.get().unwrap().components()[1]
                        - 0.001)
                        .abs()
                        < 1e-12
                );
                assert_eq!(model.recent.get_untracked().len(), self.recent.get());
                self.base.set(Some(model.current()));
                self.stage.set(6);
                // SAFETY: queued keys target only our HWND. No SendInput or keyboard state mutation.
                unsafe {
                    let scan = MapVirtualKeyW(u32::from(VK_UP), MAPVK_VK_TO_VSC);
                    let key = 1 | ((scan as isize) << 16) | (1 << 24);
                    assert_ne!(PostMessageW(hwnd, WM_KEYDOWN, VK_UP as usize, key), 0);
                    assert_ne!(
                        PostMessageW(hwnd, WM_KEYUP, VK_UP as usize, key | (3_isize << 30)),
                        0
                    );
                }
            }
            6 if model.current() != self.base.get().unwrap() => {
                assert!(
                    (model.current().components()[1]
                        - self.base.get().unwrap().components()[1]
                        - 0.001)
                        .abs()
                        < 1e-12
                );
                assert_eq!(model.recent.get_untracked().len(), self.recent.get() + 1);
                self.base.set(Some(model.current()));
                self.location.set([rect.left + 20, rect.top + 10]);
                self.stage.set(7);
                // SAFETY: move/resize this test window only. No monitor or system settings change.
                unsafe {
                    assert_ne!(
                        SetWindowPos(
                            hwnd,
                            std::ptr::null_mut(),
                            rect.left + 20,
                            rect.top + 10,
                            rect.right - rect.left + 40,
                            rect.bottom - rect.top + 30,
                            SWP_NOZORDER | SWP_NOACTIVATE
                        ),
                        0
                    );
                }
            }
            7 if client.right == self.extent.get()[0] + 40
                && client.bottom == self.extent.get()[1] + 30
                && self.presented.get() == [client.right, client.bottom] =>
            {
                assert_eq!([rect.left, rect.top], self.location.get());
                assert_eq!(model.current(), self.base.get().unwrap());
                self.stage.set(8);
                // SAFETY: exercise the normal close-request path on this owned window.
                unsafe {
                    assert_ne!(PostMessageW(hwnd, WM_SYSCOMMAND, SC_CLOSE as usize, 0), 0);
                }
            }
            _ => {}
        }
        if self.stage.get() < 8 {
            window.request_frame();
        }
    }
}

#[test]
#[ignore = "requires Windows and DX12; exercises only a temporary owned window"]
fn native_pointer_wheel_key_and_resize_reach_the_picker() {
    let (finish, wait) = std::sync::mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if matches!(
            wait.recv_timeout(Duration::from_secs(30)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ) {
            eprintln!("Native input test timed out");
            std::process::exit(1);
        }
    });
    let _dispatcher = okcolor_app::windows_dispatcher::DispatcherRuntime::new().unwrap();
    let state = Rc::new(Cell::new(None));
    let mounted = state.clone();
    let presented = Rc::new(Cell::new([0; 2]));
    let probe = Rc::new(Probe {
        state,
        stage: Cell::new(0),
        frames: Cell::new(0),
        base: Cell::new(None),
        recent: Cell::new(0),
        extent: Cell::new([0; 2]),
        location: Cell::new([0; 2]),
        presented: presented.clone(),
    });
    let mut renderer = graphics::factory();
    let renderer: zgui::runtime::RendererFactory = Box::new(move |surface, target| {
        Ok(Box::new(ObservePresentation {
            inner: renderer(surface, target)?,
            on_draw: {
                let presented = presented.clone();
                Rc::new(move |outcome, target| {
                    if matches!(outcome, zgui::render::FrameOutcome::Presented(_)) {
                        presented.set([target.size.width, target.size.height]);
                    }
                })
            },
        }))
    });
    app()
        .with_title("OkColorPicker native input test")
        .with_size(330.0, 560.0)
        .with_decorations(Decorations::NoTitleBar)
        .with_transparent(true)
        .with_fonts(fonts::load())
        .with_stylesheet(STYLE)
        .with_renderer(renderer)
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
                let model = PickerModel::default();
                let refs = PickerRefs::default();
                mounted.set(Some((model, refs)));
                view! { Picker(model=model,refs=refs) }
            },
        )
        .unwrap();
    assert_eq!(
        probe.stage.get(),
        8,
        "all native input and resize stages must complete"
    );
    finish.send(()).unwrap();
    watchdog.join().unwrap();
}
