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
    Graphics::Gdi::{
        ClientToScreen, DC_BRUSH, EnumDisplayMonitors, FillRect, GetDC, GetPixel, GetStockObject,
        HDC, HMONITOR, ReleaseDC, ScreenToClient, SetDCBrushColor, UpdateWindow,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{GetCapture, MAPVK_VK_TO_VSC, MapVirtualKeyW, VK_ESCAPE, VK_UP},
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

fn click(hwnd: HWND, node: NodeRef) {
    let point = position(node, 0.5, 0.5);
    for message in [WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP] {
        mouse(hwnd, message, point);
    }
}

// The monitor rectangles and the fixture window DPI come from the current desktop.
fn verify_monitor_samples(owner: HWND, patch: HWND, preview: HWND) -> POINT {
    unsafe extern "system" fn collect(_: HMONITOR, _: HDC, rect: *mut RECT, data: LPARAM) -> i32 {
        unsafe {
            (&mut *(data as *mut Vec<RECT>)).push(*rect);
        }
        1
    }
    let mut monitors = Vec::<RECT>::new();
    assert_ne!(
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(collect),
                &raw mut monitors as LPARAM,
            )
        },
        0
    );
    assert!(!monitors.is_empty());
    let mut last = POINT::default();
    for m in monitors {
        for at_edge in [false, true] {
            let patch_color = if at_edge { 0x95c73a } else { 0xf16693 };
            let (left, top) = if at_edge {
                (m.right - 104, m.bottom - 70)
            } else {
                (m.left + 32, m.top + 32)
            };
            let screen = if at_edge {
                POINT {
                    x: m.right - 3,
                    y: m.bottom - 3,
                }
            } else {
                POINT {
                    x: left + 3,
                    y: top + 3,
                }
            };
            unsafe {
                assert_ne!(
                    SetWindowPos(patch, HWND_TOPMOST, left, top, 104, 70, SWP_NOACTIVATE),
                    0
                );
                UpdateWindow(patch);
                let dc = GetDC(patch);
                SetDCBrushColor(dc, patch_color);
                FillRect(
                    dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: 104,
                        bottom: 70,
                    },
                    GetStockObject(DC_BRUSH),
                );
                ReleaseDC(patch, dc);
                windows_sys::Win32::Graphics::Dwm::DwmFlush();
            }
            let dpi = unsafe { GetDpiForWindow(patch) };
            assert_ne!(dpi, 0);
            let px = |n: i32| (n * dpi as i32 + 48) / 96;
            let mut point = screen;
            assert_ne!(unsafe { ScreenToClient(owner, &mut point) }, 0);
            mouse(owner, WM_MOUSEMOVE, point);
            let mut chip = RECT::default();
            unsafe {
                GetWindowRect(preview, &mut chip);
                UpdateWindow(preview);
            }
            let (x, y) = if at_edge {
                (screen.x - px(5) - px(82), screen.y - px(32))
            } else {
                (screen.x + px(5), screen.y + px(10))
            };
            assert_eq!(
                (
                    chip.left,
                    chip.top,
                    chip.right - chip.left,
                    chip.bottom - chip.top
                ),
                (x, y, px(82), px(32)),
                "desktop preview at {}x{} ({dpi} DPI)",
                screen.x,
                screen.y
            );
            let factor = f64::from(dpi) / 96.0;
            check_preview_pixels(preview, chip, factor, patch_color);
            eprintln!(
                "Desktop eyedropper at ({}, {}), DPI {dpi}, chip {}x{}",
                screen.x,
                screen.y,
                chip.right - chip.left,
                chip.bottom - chip.top
            );
            last = screen;
        }
    }
    last
}

fn check_preview_pixels(preview: HWND, chip: RECT, factor: f64, expected: u32) {
    let dc = unsafe { GetDC(preview) };
    let color = unsafe { GetPixel(dc, (12.0 * factor) as i32, (12.0 * factor) as i32) };
    assert_eq!(unsafe { GetPixel(dc, 0, 0) }, 0x424242, "preview frame");
    assert_eq!(
        unsafe { GetPixel(dc, (30.0 * factor) as i32, (16.0 * factor) as i32) },
        0,
        "gap between swatch and HEX"
    );
    let mut ink_bounds = (i32::MAX, i32::MAX, 0, 0);
    for y in (4.0 * factor) as i32..(28.0 * factor) as i32 {
        for x in (31.0 * factor) as i32..(80.0 * factor) as i32 {
            if unsafe { GetPixel(dc, x, y) } as u8 > 80 {
                ink_bounds = (
                    ink_bounds.0.min(x),
                    ink_bounds.1.min(y),
                    ink_bounds.2.max(x),
                    ink_bounds.3.max(y),
                );
            }
        }
    }
    let span = f64::from(ink_bounds.2 - ink_bounds.0 + 1) / factor;
    assert!(
        (35.0..=46.0).contains(&span),
        "six Martian Mono glyphs at 10px, bounds {ink_bounds:?}, scale {factor}"
    );
    if let Ok(dir) = std::env::var("OKCOLOR_TOOLS_CAPTURE_DIR") {
        let (width, height) = (chip.right - chip.left, chip.bottom - chip.top);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        let mut text_ink = 0;
        for y in 0..height {
            for x in 0..width {
                let pixel = unsafe { GetPixel(dc, x, y) };
                pixels.extend_from_slice(&[
                    pixel as u8,
                    (pixel >> 8) as u8,
                    (pixel >> 16) as u8,
                    255,
                ]);
                if f64::from(x) > 32.0 * factor
                    && x < width - 2
                    && y > 2
                    && y < height - 2
                    && pixel as u8 > 100
                {
                    text_ink += 1;
                }
            }
        }
        assert!(text_ink > 24, "the HEX label must be painted");
        std::fs::write(
            std::path::Path::new(&dir).join(format!("eyedropper-{width}x{height}.rgba")),
            pixels,
        )
        .unwrap();
    }
    unsafe {
        ReleaseDC(preview, dc);
    }
    assert_eq!(
        color, expected,
        "preview shows the external test window's pixel"
    );
}

struct Probe {
    caption: okcolor_app::windows_caption::CaptionBridge,
    state: Rc<Cell<Option<(PickerModel, PickerRefs)>>>,
    stage: Cell<u8>,
    sample_window: Cell<HWND>,
    frames: Cell<usize>,
    base: Cell<Option<okcolor_core::Oklch>>,
    recent: Cell<usize>,
    extent: Cell<[i32; 2]>,
    location: Cell<[i32; 2]>,
    presented: Rc<Cell<[i32; 2]>>,
}

impl FrameProbe for Probe {
    fn frame_ended(&self, window: &Window) {
        self.caption.frame_ended(window);
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
        let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
        let fitted = refs
            .settings_content
            .window_bounds()
            .or_else(|| refs.page_content.window_bounds())
            .is_some_and(|body| {
                (body.origin.y.0 + body.size.height.0 + 32.0 * scale - client.bottom as f32).abs()
                    <= 1.0
            });
        match self.stage.get() {
            0 if fitted => {
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
                self.extent.set([client.right, client.bottom]);
                self.location.set([rect.left + 20, rect.top + 10]);
                self.stage.set(7);
                // SAFETY: move/resize this test window only. No monitor or system settings change.
                unsafe {
                    let mut proposed = rect;
                    proposed.right += 40;
                    proposed.bottom += 30;
                    SendMessageW(hwnd, WM_ENTERSIZEMOVE, 0, 0);
                    SendMessageW(
                        hwnd,
                        WM_SIZING,
                        WMSZ_BOTTOMRIGHT as usize,
                        &raw mut proposed as LPARAM,
                    );
                    assert_ne!(
                        SetWindowPos(
                            hwnd,
                            std::ptr::null_mut(),
                            rect.left + 20,
                            rect.top + 10,
                            proposed.right - proposed.left,
                            proposed.bottom - proposed.top,
                            SWP_NOZORDER | SWP_NOACTIVATE
                        ),
                        0
                    );
                }
            }
            7 if fitted
                && client.right == self.extent.get()[0] + 40
                && client.bottom == self.extent.get()[1] + 30
                && self.presented.get() == [client.right, client.bottom] =>
            {
                unsafe {
                    SendMessageW(hwnd, WM_EXITSIZEMOVE, 0, 0);
                }
                assert!(
                    (model.tools.get_untracked().plot_height - 152.0 - 30.0 / scale).abs() < 1.0
                );
                assert_eq!([rect.left, rect.top], self.location.get());
                assert_eq!(model.current(), self.base.get().unwrap());
                self.stage.set(8);
                click(hwnd, refs.menu);
            }
            8 if fitted && refs.topmost.window_bounds().is_some() => {
                assert_eq!(
                    client.bottom,
                    self.extent.get()[1] + 30,
                    "Settings covers the existing client area"
                );
                self.stage.set(9);
                click(hwnd, refs.topmost);
            }
            9 if model.tools.get_untracked().always_on_top => {
                assert_ne!(
                    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_TOPMOST as isize,
                    0
                );
                self.stage.set(10);
                click(hwnd, refs.topmost);
            }
            10 if !model.tools.get_untracked().always_on_top => {
                assert_eq!(
                    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_TOPMOST as isize,
                    0
                );
                self.stage.set(11);
                click(hwnd, refs.menu);
            }
            11 if fitted && refs.eyedropper.window_bounds().is_some() => {
                assert!(
                    (refs.slice.window_bounds().unwrap().size.height.0 / scale
                        - model.tools.get_untracked().plot_height)
                        .abs()
                        <= 1.0
                );
                let external = unsafe {
                    CreateWindowExW(
                        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                        windows_sys::w!("STATIC"),
                        windows_sys::w!("OkColorPicker desktop test patch"),
                        WS_POPUP | WS_VISIBLE,
                        rect.left,
                        rect.top,
                        104,
                        70,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                assert!(!external.is_null());
                self.sample_window.set(external);
                self.stage.set(12);
                click(hwnd, refs.eyedropper);
            }
            12 if refs.sampling.get_untracked() => {
                let preview = unsafe {
                    FindWindowW(
                        windows_sys::w!("STATIC"),
                        windows_sys::w!("OkColorPicker eyedropper"),
                    )
                };
                assert!(!preview.is_null());
                assert_eq!(unsafe { GetWindow(preview, GW_OWNER) }, hwnd);
                let screen = verify_monitor_samples(hwnd, self.sample_window.get(), preview);
                // Feed the preview's queued hook messages without injecting OS mouse clicks.
                unsafe {
                    SendMessageW(preview, WM_APP + 2, screen.x as usize, screen.y as isize);
                    assert!(
                        refs.sampling.get_untracked(),
                        "sampling stays active through button-up"
                    );
                    SendMessageW(preview, WM_APP + 3, screen.x as usize, screen.y as isize);
                }
                self.stage.set(13);
            }
            13 if !refs.sampling.get_untracked() => {
                assert!(unsafe { GetCapture() }.is_null());
                assert_eq!(
                    model.output_untracked(okcolor_core::ColorFormat::Hex),
                    "#3ac795"
                );
                unsafe {
                    DestroyWindow(self.sample_window.replace(std::ptr::null_mut()));
                }
                self.stage.set(14);
                click(hwnd, refs.eyedropper);
            }
            14 if refs.sampling.get_untracked() => {
                self.stage.set(15);
                unsafe {
                    SendMessageW(hwnd, WM_KEYDOWN, VK_ESCAPE as usize, 0);
                }
            }
            15 if !refs.sampling.get_untracked() => {
                assert!(unsafe { GetCapture() }.is_null());
                assert_eq!(
                    model.output_untracked(okcolor_core::ColorFormat::Hex),
                    "#3ac795"
                );
                self.stage.set(16);
                unsafe {
                    ShowWindow(hwnd, SW_MAXIMIZE);
                }
            }
            16 if fitted && unsafe { IsZoomed(hwnd) } != 0 => {
                assert!(
                    refs.slice.window_bounds().unwrap().size.height.0 / scale
                        > model.tools.get_untracked().plot_height
                );
                self.stage.set(17);
                unsafe {
                    ShowWindow(hwnd, SW_RESTORE);
                }
            }
            17 if fitted && unsafe { IsZoomed(hwnd) } == 0 => {
                assert!(
                    (refs.slice.window_bounds().unwrap().size.height.0 / scale
                        - model.tools.get_untracked().plot_height)
                        .abs()
                        <= 1.0
                );
                self.stage.set(18);
                click(hwnd, refs.tabs[1]);
            }
            18 if fitted && refs.palette_adjust.window_bounds().is_some() => {
                click(hwnd, refs.palette_adjust);
                self.stage.set(19);
            }
            19 if refs.adjustment_rows[0].window_bounds().is_some() => {
                let point = position(refs.adjustment_rows[0], 0.5, 0.5);
                mouse(hwnd, WM_MOUSEMOVE, point);
                mouse(hwnd, WM_LBUTTONDOWN, point);
                assert_eq!(
                    unsafe { GetCapture() },
                    hwnd,
                    "native press owns OS mouse capture"
                );
                let outside = POINT {
                    x: client.right + 80,
                    y: point.y,
                };
                mouse(hwnd, WM_MOUSEMOVE, outside);
                mouse(hwnd, WM_LBUTTONUP, outside);
                self.stage.set(20);
            }
            20 if refs.adjustment_rows[0].text_content().contains('+') => {
                self.base.set(Some(model.palette.get_untracked()[0].0));
                mouse(
                    hwnd,
                    WM_MOUSEMOVE,
                    position(refs.adjustment_rows[0], 0.05, 0.5),
                );
                self.stage.set(21);
            }
            21 => {
                assert_eq!(
                    Some(model.palette.get_untracked()[0].0),
                    self.base.get(),
                    "returning after an off-window release cannot scrub"
                );
                click(hwnd, refs.adjustment_cancel);
                self.stage.set(22);
            }
            22 if refs.adjustment_menu.get_untracked().is_none() => {
                self.stage.set(23);
                // SAFETY: exercise the normal close-request path on this owned window.
                unsafe {
                    assert_ne!(PostMessageW(hwnd, WM_SYSCOMMAND, SC_CLOSE as usize, 0), 0);
                }
            }
            _ => {}
        }
        if self.stage.get() < 23 {
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
    let caption = okcolor_app::windows_caption::CaptionBridge::new(false);
    let probe = Rc::new(Probe {
        caption: caption.clone(),
        state,
        stage: Cell::new(0),
        sample_window: Cell::new(std::ptr::null_mut()),
        frames: Cell::new(0),
        base: Cell::new(None),
        recent: Cell::new(0),
        extent: Cell::new([0; 2]),
        location: Cell::new([0; 2]),
        presented: presented.clone(),
    });
    let mut renderer = caption.wrap_renderer(graphics::factory());
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
                caption.provide_window();
                mounted.set(Some((model, refs)));
                view! { Picker(model=model,refs=refs) }
            },
        )
        .unwrap();
    assert_eq!(
        probe.stage.get(),
        23,
        "all native input and resize stages must complete"
    );
    finish.send(()).unwrap();
    watchdog.join().unwrap();
}
