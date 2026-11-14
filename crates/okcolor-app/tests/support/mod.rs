//! Readback adapter adapted from zgui's Apache-2.0 test pattern.
//! Copyright 2026 Leonard Seibold.
use std::sync::{Arc, Mutex};
use zgui::render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Pixels, WgpuRenderer};

pub struct Capture {
    pub renderer: WgpuRenderer,
    pub frames: Arc<Mutex<Vec<Pixels>>>,
}

impl Renderer for Capture {
    fn target(&self) -> Option<RenderTarget> {
        self.renderer.target()
    }
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        self.renderer.capabilities()
    }
    fn configure(&mut self, target: RenderTarget) {
        self.renderer.configure(target);
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
    fn memory(&self) -> zgui::render::MemoryReport {
        self.renderer.memory()
    }
    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        let result = self.renderer.draw(scene, damage);
        self.frames
            .lock()
            .unwrap()
            .push(self.renderer.read_presented().expect("GPU readback"));
        result
    }
}
