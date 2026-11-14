//! Destroy only this test application's device and verify the mounted UI survives.
#![cfg(target_os = "windows")]
use okcolor_app::{
    display::DisplayBridge,
    fonts, graphics,
    model::PickerModel,
    persistence::SavedState,
    ui::{PickerProps, PickerRefs, STYLE},
    windows_dispatcher::DispatcherRuntime,
};
use okcolor_core::Oklch;
use raw_window_handle::RawWindowHandle;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Weak},
    time::Duration,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
use zgui::{
    prelude::*,
    render::*,
    runtime::{FrameProbe, Window},
};
use zgui_platform_winit::{UserEvent, WinitApp};
use zgui_render_wgpu::{Gpu, WgpuRenderer, wgpu};
#[path = "support/float_pixels.rs"]
mod float_pixels;
#[path = "support/observe_renderer.rs"]
mod observe_renderer;

struct Snapshot {
    saved: SavedState,
    preview: Oklch,
    pixels: Vec<[f32; 4]>,
}

struct Probe {
    outcome: Rc<Cell<Option<FrameOutcome>>>,
    model: Rc<Cell<Option<PickerModel>>>,
    baseline: RefCell<Option<Snapshot>>,
    old_gpu: RefCell<Option<Weak<Gpu>>>,
    recovered: Cell<u32>,
    destroyed: Cell<u32>,
    finished: Cell<bool>,
    destroy_worker: RefCell<Option<std::thread::JoinHandle<()>>>,
}

impl FrameProbe for Probe {
    fn frame_ended(&self, window: &Window) {
        let outcome = self.outcome.take();
        if matches!(outcome, Some(FrameOutcome::Recovered)) {
            self.recovered.set(self.recovered.get() + 1);
            return;
        }
        if !matches!(outcome, Some(FrameOutcome::Presented(_)))
            || self.finished.get()
            || self.destroyed.get() > self.recovered.get()
        {
            return;
        }
        if let Some(worker) = self.destroy_worker.borrow_mut().take() {
            worker.join().expect("Device destruction worker");
        }
        let renderer = window
            .renderer()
            .as_any()
            .unwrap()
            .downcast_ref::<WgpuRenderer>()
            .unwrap();
        let gpu = renderer.gpu();
        let pixels = float_pixels::read_float(gpu, renderer.offscreen_texture().unwrap());
        let model = self.model.get().unwrap();
        if let Some(Snapshot {
            saved,
            preview,
            pixels: expected,
        }) = self.baseline.borrow().as_ref()
        {
            assert_eq!(model.saved_state_untracked(), *saved);
            assert_eq!(model.current(), *preview, "An active drag must survive");
            assert_eq!(pixels.len(), expected.len());
            let changed = pixels
                .iter()
                .zip(expected)
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| (a - b).abs() > 0.005))
                .count();
            assert_eq!(
                changed, 0,
                "Restored pixels, including glyphs and custom paint, must match"
            );
            assert!(
                self.old_gpu.borrow().as_ref().unwrap().upgrade().is_none(),
                "Old device must be released"
            );
        }
        if self.destroyed.get() == 2 {
            assert_eq!(self.recovered.get(), 2);
            let committed = model.saved_state_untracked().current;
            model.cancel();
            assert_eq!(model.current(), committed);
            model.undo();
            assert_ne!(model.current(), committed);
            model.redo();
            assert_eq!(model.current(), committed);
            self.finished.set(true);
            let native = window.surface().clone().gpu_shared().unwrap();
            let RawWindowHandle::Win32(handle) = native.window_handle().unwrap().as_raw() else {
                panic!("HWND")
            };
            // SAFETY: close only the HWND owned by this test, via its normal event loop.
            unsafe { PostMessageW(handle.hwnd.get() as _, WM_CLOSE, 0, 0) };
            return;
        }
        if self.baseline.borrow().is_none() {
            *self.baseline.borrow_mut() = Some(Snapshot {
                saved: model.saved_state_untracked(),
                preview: model.current(),
                pixels,
            });
        }
        *self.old_gpu.borrow_mut() = Some(Arc::downgrade(gpu));
        self.destroyed.set(self.destroyed.get() + 1);
        // Let the UI park first. The background callback must wake it without
        // a model change, input event or manual redraw from this test.
        let gpu = gpu.clone();
        *self.destroy_worker.borrow_mut() = Some(std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            // This destroys only our own logical device, not the display driver.
            gpu.device().destroy();
            let _ = gpu.device().poll(wgpu::PollType::Poll);
            assert!(
                gpu.loss().is_lost(),
                "The real wgpu loss callback must record destruction"
            );
        }));
    }
}

#[test]
#[ignore = "requires a Windows desktop and DX12 GPU; destroys only its own logical devices"]
fn native_device_loss_restores_pixels_and_preserves_edit_and_history_twice() {
    let (finish, wait) = std::sync::mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if matches!(
            wait.recv_timeout(Duration::from_secs(45)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ) {
            eprintln!("Native device recovery timed out");
            std::process::exit(1);
        }
    });
    let _dispatcher = DispatcherRuntime::new().unwrap();
    let outcome = Rc::new(Cell::new(None));
    let model = Rc::new(Cell::new(None));
    let probe = Rc::new(Probe {
        outcome: outcome.clone(),
        model: model.clone(),
        baseline: RefCell::new(None),
        old_gpu: RefCell::new(None),
        recovered: Cell::new(0),
        destroyed: Cell::new(0),
        finished: Cell::new(false),
        destroy_worker: RefCell::new(None),
    });
    let display = DisplayBridge::default();
    let mut factory = graphics::factory_with_display(display.clone());
    app()
        .with_title("OkColorPicker device recovery test")
        .with_size(330.0, 560.0)
        .with_decorations(Decorations::NoTitleBar)
        .with_transparent(true)
        .with_fonts(fonts::load())
        .with_stylesheet(STYLE)
        .with_probe(probe.clone())
        .with_renderer(Box::new(move |surface, target| {
            let outcome = outcome.clone();
            Ok(Box::new(observe_renderer::ObservePresentation {
                inner: factory(surface, target)?,
                on_draw: Rc::new(move |result, _| {
                    outcome.set(Some(result));
                }),
            }))
        }))
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
                let picker = PickerModel::default();
                picker.select(Oklch::new(0.71, 0.19, 147.123456789, 0.37).unwrap());
                assert!(picker.begin());
                picker.preview(Oklch::new(0.63, 0.23, 284.123456789, 0.37).unwrap());
                display.bind(picker.display);
                model.set(Some(picker));
                view! { Picker(model=picker,refs=PickerRefs::default()) }
            },
        )
        .unwrap();
    assert!(probe.finished.get());
    finish.send(()).unwrap();
    watchdog.join().unwrap();
}
