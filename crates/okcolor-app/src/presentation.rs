//! Keep zgui composition in FP16 and convert only at the native presentation boundary.
use crate::display::{DisplayBridge, DisplayMode, DisplayStatus};
use crate::{output_pass::OutputPass, windows_display_watch::DisplayWatch};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use zgui::{platform::GpuSurface, render::*};
use zgui_render_wgpu::{Acquisition, SharedGraphics, WgpuRenderer, wgpu};

pub struct Presentation {
    inner: WgpuRenderer,
    graphics: SharedGraphics,
    target: RenderTarget,
    retry_after: Option<Instant>,
    recovery_failures: u32,
    unavailable_sink: crate::unavailable_sink::UnavailableSink,
    surface: Option<wgpu::Surface<'static>>,
    native: Arc<dyn GpuSurface>,
    display: DisplayWatch,
    display_bridge: DisplayBridge,
    config: wgpu::SurfaceConfiguration,
    output: OutputPass,
    configure_pending: bool,
    present_pending: bool,
    has_frame: bool,
    acquisition_time: Duration,
    validation_failures: u32,
}

impl Presentation {
    pub fn new(
        graphics: &SharedGraphics,
        native: Arc<dyn GpuSurface>,
        target: RenderTarget,
        display_bridge: DisplayBridge,
    ) -> Result<Self, String> {
        let gpu = graphics.gpu().ok_or("No presentation device")?;
        watch_loss(&gpu, &native);
        let display = DisplayWatch::new(native.clone()).map_err(|e| e.to_string())?;
        let surface = gpu
            .instance()
            .create_surface(native.clone())
            .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(gpu.adapter());
        if !caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            return Err("DirectComposition requires premultiplied surface alpha".into());
        }
        let format = select_format(&caps.formats, display.white_scale().is_some())?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: target.size.width.max(1) as u32,
            height: target.size.height.max(1) as u32,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
        };
        let mut inner = graphics
            .renderer_offscreen(target, wgpu::TextureFormat::Rgba16Float, false)
            .map_err(|e| e.to_string())?;
        zgui_render_vector_vello::attach(&mut inner, target.size);
        let output = OutputPass::new(
            &gpu,
            inner.offscreen_texture().expect("offscreen renderer"),
            format,
        );
        eprintln!("Presentation: {format:?} / PreMultiplied; FP16 gamma composition");
        Ok(Self {
            inner,
            graphics: graphics.clone(),
            target,
            retry_after: None,
            recovery_failures: 0,
            unavailable_sink: crate::unavailable_sink::UnavailableSink,
            surface: Some(surface),
            native,
            display,
            display_bridge,
            config,
            output,
            configure_pending: true,
            present_pending: false,
            has_frame: false,
            acquisition_time: Duration::ZERO,
            validation_failures: 0,
        })
    }

    fn recover(&mut self) -> FrameOutcome {
        if self
            .retry_after
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return FrameOutcome::Skipped(SkipReason::DeviceUnavailable);
        }
        // The old HWND swap chain must be released before creating its replacement.
        self.surface = None;
        self.has_frame = false;
        self.present_pending = false;
        let rebuilt = (|| -> Result<(), String> {
            if self.graphics.gpu().is_none_or(|gpu| gpu.loss().is_lost()) {
                self.graphics.adopt(crate::graphics::open_gpu()?);
            }
            self.inner.recover().map_err(|error| error.to_string())?;
            self.inner.configure(self.target);
            zgui_render_vector_vello::attach(&mut self.inner, self.target.size);
            watch_loss(self.inner.gpu(), &self.native);
            self.output = OutputPass::new(
                self.inner.gpu(),
                self.inner.offscreen_texture().unwrap(),
                self.config.format,
            );
            self.configure_pending = true;
            self.prepare_surface()
        })();
        match rebuilt {
            Ok(()) => {
                self.retry_after = None;
                self.recovery_failures = 0;
                self.validation_failures = 0;
                eprintln!("Graphics device and presentation recovered");
                FrameOutcome::Recovered
            }
            Err(error) => {
                self.inner
                    .gpu()
                    .loss()
                    .report(wgpu::DeviceLostReason::Unknown, &error);
                self.recovery_failures = self.recovery_failures.saturating_add(1);
                let delay = Duration::from_millis(match self.recovery_failures {
                    1 => 250,
                    2 => 1000,
                    3 => 2000,
                    _ => 5000,
                });
                self.retry_after = Some(Instant::now() + delay);
                eprintln!("Graphics recovery failed; retry in {delay:?}: {error}");
                let native = Arc::downgrade(&self.native);
                // Only one timer is created per failed attempt. It holds no window
                // alive and GPU work stays on the UI thread after the wake.
                std::thread::spawn(move || {
                    std::thread::sleep(delay);
                    if let Some(native) = native.upgrade() {
                        native.request_redraw();
                    }
                });
                FrameOutcome::Skipped(SkipReason::DeviceUnavailable)
            }
        }
    }

    fn prepare_surface(&mut self) -> Result<(), String> {
        let gpu = self.inner.gpu();
        if self.surface.is_none() {
            self.surface = Some(
                gpu.instance()
                    .create_surface(self.native.clone())
                    .map_err(|e| e.to_string())?,
            );
            self.configure_pending = true;
        }
        let surface = self.surface.as_ref().expect("surface recreated above");
        if self.configure_pending {
            let caps = surface.get_capabilities(gpu.adapter());
            if !caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                return Err("Surface no longer supports premultiplied alpha".into());
            }
            let format = select_format(&caps.formats, self.display.white_scale().is_some())?;
            if format != self.config.format {
                self.config.format = format;
                self.output = OutputPass::new(gpu, self.inner.offscreen_texture().unwrap(), format);
                eprintln!("Presentation format changed: {format:?}");
            }
            let scope = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
            surface.configure(gpu.device(), &self.config);
            if let Some(error) = futures::executor::block_on(scope.pop()) {
                return Err(error.to_string());
            }
            self.configure_pending = false;
        }
        Ok(())
    }

    fn present(&mut self, mut stats: FrameStats) -> FrameOutcome {
        if let Err(error) = self.prepare_surface() {
            self.inner
                .gpu()
                .loss()
                .report(wgpu::DeviceLostReason::Unknown, &error);
            return self.recover();
        }
        let began = Instant::now();
        let texture = self.surface.as_ref().unwrap().get_current_texture();
        self.acquisition_time = began.elapsed();
        let acquisition = Acquisition::classify(&texture);
        self.configure_pending |= acquisition.response().reconfigure;
        if acquisition == Acquisition::Validation {
            self.validation_failures += 1;
            if self.validation_failures >= 3 {
                self.inner.gpu().loss().report(
                    wgpu::DeviceLostReason::Unknown,
                    "Repeated presentation validation failures",
                );
                return self.recover();
            }
        } else {
            self.validation_failures = 0;
        }
        match texture {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let gpu = self.inner.gpu();
                let view = texture.texture.create_view(&Default::default());
                let mut encoder = gpu.device().create_command_encoder(&Default::default());
                self.output.encode(
                    gpu,
                    &mut encoder,
                    &view,
                    self.display.white_scale().unwrap_or(1.0),
                );
                gpu.queue().submit([encoder.finish()]);
                self.native.pre_present_notify();
                texture.present();
                self.present_pending = false;
                stats.draw_calls += 1;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = None;
            }
            _ => {}
        }
        acquisition.outcome(stats)
    }
}

fn watch_loss(gpu: &Arc<zgui_render_wgpu::Gpu>, native: &Arc<dyn GpuSurface>) {
    let observer = gpu.loss().observer();
    let native = Arc::downgrade(native);
    gpu.device()
        .set_device_lost_callback(move |reason, message| {
            observer(reason, &message);
            if let Some(native) = native.upgrade() {
                native.request_redraw();
            }
        });
}

fn select_format(
    formats: &[wgpu::TextureFormat],
    known_white: bool,
) -> Result<wgpu::TextureFormat, String> {
    [
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ]
    .into_iter()
    .find(|format| {
        (*format != wgpu::TextureFormat::Rgba16Float || known_white) && formats.contains(format)
    })
    .ok_or_else(|| "No supported scRGB or unencoded SDR surface format".into())
}

impl Renderer for Presentation {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }
    fn target(&self) -> Option<RenderTarget> {
        Some(self.target)
    }
    fn configure(&mut self, target: RenderTarget) {
        if self.target == target {
            return;
        }
        self.target = target;
        if !self.inner.gpu().loss().is_lost() && self.retry_after.is_none() {
            self.inner.configure(target);
            self.output
                .bind_source(self.inner.gpu(), self.inner.offscreen_texture().unwrap());
        }
        let width = target.size.width.max(1) as u32;
        let height = target.size.height.max(1) as u32;
        self.configure_pending |= (width, height) != (self.config.width, self.config.height);
        self.config.width = width;
        self.config.height = height;
        self.has_frame = false;
        self.present_pending = false;
    }
    fn draw(&mut self, scene: &zgui::scene::Scene, damage: &zgui::bits::DamageSet) -> FrameOutcome {
        if self.validation_failures >= 3
            || self.inner.gpu().loss().is_lost()
            || self.retry_after.is_some()
        {
            if self.validation_failures >= 3 {
                self.inner.gpu().loss().report(
                    wgpu::DeviceLostReason::Unknown,
                    "Repeated presentation validation failures",
                );
            }
            return self.recover();
        }
        if self.display.refresh() {
            self.configure_pending = true;
            self.present_pending = true;
        }
        if let Err(error) = self.prepare_surface() {
            self.inner
                .gpu()
                .loss()
                .report(wgpu::DeviceLostReason::Unknown, &error);
            return self.recover();
        }
        let mode = self.display.mode();
        let status = DisplayStatus {
            mode,
            wide_output: self.config.format == wgpu::TextureFormat::Rgba16Float
                && matches!(mode, DisplayMode::WideColor | DisplayMode::Hdr)
                && self.display.white_scale().is_some(),
        };
        if self.display_bridge.publish(status) {
            // The current scene was built for the old policy. Retain its damage
            // and let reactive paint effects update before submitting anything.
            self.has_frame = false;
            return FrameOutcome::Skipped(SkipReason::Unacquired);
        }
        let result = self.inner.draw(scene, damage);
        // Loss can arrive after the entry check, including during submission.
        if self.inner.gpu().loss().is_lost() {
            return self.recover();
        }
        let stats = match result {
            FrameOutcome::Presented(stats) => {
                self.has_frame = true;
                self.present_pending = true;
                stats
            }
            FrameOutcome::Skipped(SkipReason::Undamaged)
                if self.has_frame && self.present_pending =>
            {
                FrameStats {
                    memory: self.memory(),
                    ..Default::default()
                }
            }
            _ => return result,
        };
        self.present(stats)
    }
    fn present_held(&mut self) -> FrameOutcome {
        if self.inner.gpu().loss().is_lost() || self.retry_after.is_some() {
            return self.recover();
        }
        match self.inner.present_held() {
            FrameOutcome::Presented(stats) => {
                self.has_frame = true;
                self.present_pending = true;
                self.present(stats)
            }
            other => other,
        }
    }
    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        // Runtime flushes uploads before draw(), where device recovery happens.
        // Refuse writes to the lost device so their CPU copies remain available.
        if self.inner.gpu().loss().is_lost() || self.retry_after.is_some() {
            &mut self.unavailable_sink
        } else {
            self.inner.texture_sink()
        }
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
        self.acquisition_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn float_presentation_requires_known_reference_white_and_actual_surface_support() {
        use wgpu::TextureFormat::{Bgra8Unorm, Rgba8Unorm, Rgba16Float};
        assert_eq!(
            select_format(&[Bgra8Unorm, Rgba16Float], true).unwrap(),
            Rgba16Float
        );
        assert_eq!(
            select_format(&[Bgra8Unorm, Rgba16Float], false).unwrap(),
            Bgra8Unorm
        );
        assert_eq!(select_format(&[Rgba8Unorm], true).unwrap(), Rgba8Unorm);
        assert!(select_format(&[Rgba16Float], false).is_err());
        assert!(select_format(&[], true).is_err());
    }
}
