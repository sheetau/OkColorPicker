//! End-to-end native messages within this test's own Picker window; no desktop input injection.
#![cfg(target_os = "windows")]
use okcolor_app::{
    display::DisplayBridge,
    fonts, graphics,
    model::PickerModel,
    ui::{PickerProps, PickerRefs, STYLE},
    windows_caption::CaptionBridge,
};
use raw_window_handle::RawWindowHandle;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use zgui::render::*;

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

struct ObservePresentation {
    inner: Box<dyn Renderer>,
    deferred: Rc<Cell<u32>>,
    sizes: Rc<RefCell<Vec<zgui::geom::Size<i32, zgui::geom::Device>>>>,
}
impl Renderer for ObservePresentation {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }
    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }
    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }
    fn draw(&mut self, scene: &zgui::scene::Scene, damage: &zgui::bits::DamageSet) -> FrameOutcome {
        let result = self.inner.draw(scene, damage);
        if matches!(result, FrameOutcome::Skipped(SkipReason::Unacquired)) {
            self.deferred.set(self.deferred.get() + 1);
        }
        assert!(
            !matches!(result, FrameOutcome::Skipped(SkipReason::DeviceUnavailable)),
            "Native presentation became unavailable"
        );
        if matches!(result, FrameOutcome::Presented(_)) {
            self.sizes
                .borrow_mut()
                .push(self.inner.target().unwrap().size);
        }
        result
    }
    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.inner.texture_sink()
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        self.inner.as_any_mut()
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        self.inner.as_any()
    }
    fn shifts_composed_pixels(&self) -> bool {
        self.inner.shifts_composed_pixels()
    }
    fn composed_survives_configure(&self) -> bool {
        self.inner.composed_survives_configure()
    }
    fn shift_composed(&mut self, shift: ScrollShift) {
        self.inner.shift_composed(shift);
    }
    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }
    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }
    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }
    fn vector_status(&self) -> VectorStatus {
        self.inner.vector_status()
    }
    fn target_pool(&self) -> TargetPoolReport {
        self.inner.target_pool()
    }
    fn release_cached_targets(&mut self) -> u64 {
        self.inner.release_cached_targets()
    }
    fn release_idle_resources(&mut self) -> u64 {
        self.inner.release_idle_resources()
    }
    fn acquire_block(&self) -> Duration {
        self.inner.acquire_block()
    }
}

struct Probe {
    caption: CaptionBridge,
    button: Rc<Cell<Option<NodeRef>>>,
    stage: Cell<u8>,
    frames: Cell<u32>,
    sizes: Rc<RefCell<Vec<zgui::geom::Size<i32, zgui::geom::Device>>>>,
}

impl FrameProbe for Probe {
    fn frame_ended(&self, window: &Window) {
        self.caption.frame_ended(window);
        let frames = self.frames.get() + 1;
        self.frames.set(frames);
        assert!(frames < 60, "Native caption round trip did not settle");
        let maximized = window.surface().is_maximized();
        // Windows changes its maximized flag before winit delivers the new
        // physical extent. Wait for that extent to reach presentation before
        // sending restore, and wait for the restored frame before closing.
        let ready = {
            let sizes = self.sizes.borrow();
            match (self.stage.get(), sizes.first(), sizes.last()) {
                (_, None, _) => false,
                (2, Some(first), Some(last)) => {
                    last.width > first.width && last.height > first.height
                }
                (3, Some(first), Some(last)) => first == last,
                _ => true,
            }
        };
        if !ready {
            window.request_frame();
            return;
        }
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
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::new(Probe {
        caption: caption.clone(),
        button: button.clone(),
        stage: Cell::new(0),
        frames: Cell::new(0),
        sizes: sizes.clone(),
    });
    let recorded = sizes.clone();
    let display = DisplayBridge::default();
    let deferred = Rc::new(Cell::new(0));
    let observed_deferred = deferred.clone();
    let known_display = Rc::new(Cell::new(false));
    let observed_display = known_display.clone();
    let mut renderer = graphics::factory_with_display(display.clone());
    let renderer: zgui::runtime::RendererFactory = Box::new(move |surface, target| {
        let handles = surface.clone().gpu_shared().unwrap();
        observed_display.set(
            okcolor_app::windows_display::query(handles.as_ref()).is_ok_and(|color| {
                color.mode != okcolor_app::windows_display::ColorMode::AdvancedUnknown
            }),
        );
        Ok(Box::new(ObservePresentation {
            inner: renderer(surface, target)?,
            deferred: observed_deferred.clone(),
            sizes: recorded.clone(),
        }))
    });
    app()
        .with_title("OkColorPicker native caption test")
        .with_size(330.0, 560.0)
        .with_min_size(330.0, 520.0)
        .with_decorations(Decorations::NoTitleBar)
        .with_transparent(true)
        .with_fonts(fonts::load())
        .with_stylesheet(STYLE)
        .with_renderer(caption.wrap_renderer(renderer))
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
                let model = PickerModel::default();
                display.bind(model.display);
                view! { Picker(model=model,refs=refs) }
            },
        )
        .unwrap();
    assert_eq!(
        probe.stage.get(),
        4,
        "Cancellation, maximize and restore must finish"
    );
    let sizes = sizes.borrow();
    if known_display.get() {
        assert!(
            deferred.get() > 0,
            "Initial display discovery must defer the old preview scene"
        );
    }
    let first = sizes
        .first()
        .expect("At least one frame must reach the native swap chain");
    assert!(
        sizes
            .iter()
            .any(|size| size.width > first.width && size.height > first.height),
        "A maximized frame must actually be presented: {sizes:?}"
    );
    assert_eq!(
        sizes.last(),
        Some(first),
        "A restored frame must actually be presented"
    );
    finish.send(()).unwrap();
    watchdog.join().unwrap();
}
