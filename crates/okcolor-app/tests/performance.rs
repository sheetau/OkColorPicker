//! Serialized offscreen workload measurements, not native input-to-display latency.
use okcolor_app::{
    fonts,
    model::PickerModel,
    output_pass::OutputPass,
    persistence::SavedState,
    ui::{PickerProps, PickerRefs, STYLE},
};
use okcolor_core::Gamut;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Instant,
};
use zgui::{
    geom::{CssPx, DevicePx, Point, Size},
    platform::SurfaceEvent,
    prelude::*,
    render::{FrameOutcome, MemoryReport, RenderTarget, Renderer},
    vocab::{PointerAction, PointerEvent, Timestamp},
};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{
    Gpu, SharedGraphics, WgpuRenderer,
    gpu::{adapter, timing},
    wgpu,
};

#[derive(Default)]
struct Measurements {
    frames: usize,
    gpu_ms: Vec<f64>,
    memory: MemoryReport,
    output_bytes: u64,
}

struct MeasuredRenderer {
    renderer: WgpuRenderer,
    output: Option<(wgpu::Texture, OutputPass)>,
    measurements: Rc<RefCell<Measurements>>,
}

impl Renderer for MeasuredRenderer {
    fn target(&self) -> Option<RenderTarget> {
        self.renderer.target()
    }
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        self.renderer.capabilities()
    }
    fn shifts_composed_pixels(&self) -> bool {
        self.renderer.shifts_composed_pixels()
    }
    fn composed_survives_configure(&self) -> bool {
        self.renderer.composed_survives_configure()
    }
    fn shift_composed(&mut self, shift: zgui::render::ScrollShift) {
        self.renderer.shift_composed(shift);
    }
    fn vector_status(&self) -> zgui::render::VectorStatus {
        self.renderer.vector_status()
    }
    fn target_pool(&self) -> zgui::render::TargetPoolReport {
        self.renderer.target_pool()
    }
    fn release_cached_targets(&mut self) -> u64 {
        self.renderer.release_cached_targets()
    }
    fn release_idle_resources(&mut self) -> u64 {
        self.renderer.release_idle_resources()
    }
    fn as_any(&self) -> Option<&dyn core::any::Any> {
        self.renderer.as_any()
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.renderer.as_any_mut()
    }
    fn configure(&mut self, target: RenderTarget) {
        self.renderer.configure(target);
        self.output = None;
    }
    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.renderer.texture_sink()
    }
    fn register_external(
        &mut self,
        texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        self.renderer.register_external(texture)
    }
    fn release_external(&mut self, handle: zgui::render::TextureHandle) {
        self.renderer.release_external(handle);
    }
    fn memory(&self) -> MemoryReport {
        self.renderer.memory()
    }
    fn draw(&mut self, scene: &zgui::scene::Scene, damage: &zgui::bits::DamageSet) -> FrameOutcome {
        let result = self.renderer.draw(scene, damage);
        if matches!(result, FrameOutcome::Presented(_)) {
            let gpu = self.renderer.gpu();
            let source = self.renderer.offscreen_texture().unwrap();
            let (output, pass) = self.output.get_or_insert_with(|| {
                let output = gpu.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("measurement scRGB output"),
                    size: source.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let pass = OutputPass::new(gpu, source, output.format());
                (output, pass)
            });
            let mut encoder = gpu.device().create_command_encoder(&Default::default());
            pass.encode(
                gpu,
                &mut encoder,
                &output.create_view(&Default::default()),
                1.0,
            );
            gpu.queue().submit([encoder.finish()]);
            // Serialize this workload deliberately: no queued-frame overlap or pixel readback.
            gpu.device()
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("GPU completion");
            let mut measured = self.measurements.borrow_mut();
            measured.frames += 1;
            measured.memory = self.renderer.memory();
            measured.output_bytes = u64::from(output.width()) * u64::from(output.height()) * 8;
            for time in self.renderer.take_gpu_timings() {
                assert_eq!(time.untimed, 0, "incomplete GPU timing");
                measured.gpu_ms.push(time.total_ms());
            }
        }
        result
    }
}

#[cfg(windows)]
fn report_private_commit(phase: &str) {
    use windows_sys::Win32::System::Threading::{
        APP_MEMORY_INFORMATION, GetCurrentProcess, GetProcessInformation, ProcessAppMemoryInfo,
    };
    let mut memory = APP_MEMORY_INFORMATION::default();
    // SAFETY: the SDK writes this sized structure synchronously for the current process.
    let ok = unsafe {
        GetProcessInformation(
            GetCurrentProcess(),
            ProcessAppMemoryInfo,
            (&raw mut memory).cast(),
            size_of_val(&memory) as u32,
        )
    };
    assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
    println!(
        "{}",
        json!({"phase": phase, "process_private_commit_bytes": memory.PrivateCommitUsage})
    );
}

fn distribution(values: &[f64]) -> serde_json::Value {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({"samples": sorted.len(), "p50_ms": sorted[(sorted.len() - 1) / 2],
           "p95_ms": sorted[(sorted.len() * 95).div_ceil(100) - 1],
           "max_ms": sorted.last().unwrap()})
}

#[test]
#[ignore = "measurement requires --release, DX12 and timestamp queries; run alone with --nocapture"]
fn measure_picker_drag_workloads() {
    #[allow(
        clippy::assertions_on_constants,
        reason = "compile in debug suites but reject debug measurements at runtime"
    )]
    {
        assert!(!cfg!(debug_assertions), "measure a release build");
    }
    #[cfg(windows)]
    report_private_commit("before_fonts");
    let fonts = fonts::load();
    #[cfg(windows)]
    report_private_commit("after_fonts");
    timing::enable();
    let mut descriptor = adapter::instance_descriptor(wgpu::Backends::DX12);
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::StaticDxc;
    let instance = wgpu::Instance::new(descriptor);
    #[cfg(windows)]
    report_private_commit("after_instance");
    let adapters = adapter::candidates(&instance, wgpu::Backends::DX12);
    #[cfg(windows)]
    report_private_commit("after_adapter_enumeration");
    let gpu = Arc::new(
        adapters
            .into_iter()
            .find_map(|adapter| Gpu::open(instance.clone(), adapter, &[]).ok())
            .expect("DX12 device"),
    );
    println!(
        "{}",
        json!({"adapter": gpu.adapter().get_info().name,
        "scope": "headless DX12 FP16 + scRGB; serialized completion; no native presentation",
        "gpu_scope": "zgui timed passes only; excludes final scRGB pass and copies; vector spans are upper bounds"})
    );
    #[cfg(windows)]
    report_private_commit("after_device_open");
    let graphics = SharedGraphics::with_gpu(gpu);
    let measurements = Rc::new(RefCell::new(Measurements::default()));
    let capture = measurements.clone();
    let state = Rc::new(Cell::new(None));
    let mounted = state.clone();
    app().with_size(330.0, 560.0).with_transparent(true).with_fonts(fonts)
        .with_stylesheet(STYLE)
        .with_renderer(Box::new(move |_, target| {
            let mut renderer = graphics.renderer_offscreen(target, wgpu::TextureFormat::Rgba16Float, false)?;
            assert!(renderer.gpu_timing_enabled(), "timestamp queries required");
            zgui_render_vector_vello::attach(&mut renderer, target.size);
            Ok(Box::new(MeasuredRenderer { renderer, output: None, measurements: capture.clone() }) as Box<dyn Renderer>)
        }))
        .run_on(|handler| {
            let mut harness = Harness::new(handler);
            harness.settle(64);
            #[cfg(windows)]
            report_private_commit("after_picker_settle");
            let startup = measurements.borrow().memory;
            println!("{}", json!({"phase": "settled_picker", "scale": 1.0,
                "renderer_bytes": {"fixed": startup.fixed, "targets": startup.targets,
                "scratch": startup.scratch, "atlases": startup.atlases, "buffers": startup.buffers},
                "scRGB_target_bytes": measurements.borrow().output_bytes}));
            let (model, refs): (PickerModel, PickerRefs) = state.get().unwrap();
            for scale in [1.0, 2.0] {
                harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                    scale_factor: scale, size: Size::new(DevicePx(330.0 * scale as f32), DevicePx(560.0 * scale as f32)),
                });
                // Commit the coalesced resize before reading device-pixel bounds.
                harness.advance(std::time::Duration::from_millis(20));
                harness.settle(64);
                for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
                    model.gamut.set(gamut);
                    harness.settle(64);
                    for (label, node) in [("H", refs.bars[2]), ("LC", refs.slice)] {
                        model.select(okcolor_core::Oklch::new(0.6, 0.08, 210.0, 1.0).unwrap());
                        harness.settle(32);
                        let bounds = node.window_bounds().unwrap();
                        let pointer = |action, step: usize| {
                            let t = (step % 120) as f32 / 119.0;
                            let (x, y) = if label == "LC" { (0.02 + t * 0.08, 0.35 + t * 0.3) }
                                         else { (0.1 + t * 0.8, 0.5) };
                            SurfaceEvent::Pointer { action, event: PointerEvent::mouse(Point::new(
                                CssPx((bounds.origin.x.0 + bounds.size.width.0 * x) / scale as f32),
                                CssPx((bounds.origin.y.0 + bounds.size.height.0 * y) / scale as f32),
                            )).with_button(PointerButton::Primary), modifiers: Modifiers::NONE, timestamp: Timestamp::ORIGIN }
                        };
                        harness.deliver_to_first(pointer(PointerAction::Pressed, 0));
                        harness.settle(32);
                        // Warm caches and pipeline state separately for every workload.
                        for step in 0..20 {
                            harness.deliver_to_first(pointer(PointerAction::Moved, step));
                            harness.settle(32);
                        }
                        measurements.borrow_mut().gpu_ms.clear();
                        let start_frames = measurements.borrow().frames;
                        let mut wall_ms = Vec::new();
                        for step in 20..140 {
                            let before = model.current();
                            let start = Instant::now();
                            harness.deliver_to_first(pointer(PointerAction::Moved, step));
                            harness.settle(32);
                            wall_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                            assert_ne!(model.current(), before, "{label} drag did not update the model");
                        }
                        {
                            let measured = measurements.borrow();
                            let frames = measured.frames - start_frames;
                            assert!(frames >= 120, "{label}: missing rendered updates");
                            assert_eq!(measured.gpu_ms.len(), frames, "missing GPU samples");
                            println!("{}", json!({"control": label, "gamut": format!("{gamut:?}"), "scale": scale,
                                "frames": frames, "dispatch_to_gpu_complete": distribution(&wall_ms),
                                "zgui_gpu_passes": distribution(&measured.gpu_ms),
                                "renderer_accounted_bytes": measured.memory.total(), "scRGB_target_bytes": measured.output_bytes}));
                        }
                        harness.deliver_to_first(pointer(PointerAction::Released, 139));
                        harness.settle(32);
                        let idle_frames = measurements.borrow().frames;
                        for _ in 0..10 { harness.settle(32); }
                        assert_eq!(measurements.borrow().frames, idle_frames, "idle redraws");
                    }
                }
            }
            harness.shut_down();
            Ok(())
        }, move || {
            let model = PickerModel::restore(SavedState::default());
            let refs = PickerRefs::default();
            mounted.set(Some((model, refs)));
            view! { Picker(model=model,refs=refs) }
        }).expect("measurement workload");
}
