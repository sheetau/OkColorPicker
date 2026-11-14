//! Observe production renderer outcomes without replacing its behavior.
use std::{rc::Rc, time::Duration};
use zgui::render::*;
pub struct ObservePresentation {
    pub inner: Box<dyn Renderer>,
    pub on_draw: Rc<dyn Fn(FrameOutcome, RenderTarget)>,
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
        (self.on_draw)(result, self.inner.target().unwrap());
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
