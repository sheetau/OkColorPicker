//! Windows presentation policy, kept separate from color calculations and widgets.
use std::{cell::RefCell, sync::Arc};
use zgui::{platform::Surface, render::Renderer, runtime::AppError};
use zgui_render_wgpu::{Gpu, PresentPacing, SharedGraphics, gpu::adapter, wgpu};

/// Creates one device lazily, when the first hidden window has a presentation target.
/// Device loss is reported rather than silently changing backends.
pub fn factory() -> zgui::runtime::RendererFactory {
    let graphics: RefCell<Option<SharedGraphics>> = RefCell::new(None);
    Box::new(move |surface: &Arc<dyn Surface>, target| {
        let handles = Arc::clone(surface).gpu_shared().ok_or_else(|| {
            AppError::Platform(zgui::platform::PlatformError::Backend(
                "The window has no native presentation handle".into(),
            ))
        })?;
        #[cfg(target_os = "windows")]
        crate::windows_composition::verify(handles.as_ref()).map_err(|error| {
            AppError::Platform(zgui::platform::PlatformError::Backend(format!(
                "Invalid composition window: {error}"
            )))
        })?;
        #[cfg(target_os = "windows")]
        match crate::windows_display::query(handles.as_ref()) {
            Ok(display) => {
                eprintln!("Display capabilities: {display:?}; output remains sRGB preview")
            }
            Err(error) => {
                eprintln!("Display capabilities unavailable: {error}; output remains sRGB preview")
            }
        }
        if graphics.borrow().is_none() {
            let mut descriptor = adapter::instance_descriptor(wgpu::Backends::DX12);
            descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::StaticDxc;
            descriptor.backend_options.dx12.presentation_system =
                wgpu::Dx12SwapchainKind::DxgiFromVisual;
            let instance = wgpu::Instance::new(descriptor);
            let mut errors = Vec::new();
            for candidate in adapter::candidates(&instance, wgpu::Backends::DX12) {
                let name = candidate.get_info().name;
                match Gpu::open(instance.clone(), candidate, &[]) {
                    Ok(gpu) => {
                        eprintln!("Graphics device: {name}; DX12 / DirectComposition / static DXC");
                        *graphics.borrow_mut() = Some(SharedGraphics::with_gpu(Arc::new(gpu)));
                        break;
                    }
                    Err(error) => errors.push(error),
                }
            }
            if graphics.borrow().is_none() {
                return Err(AppError::Platform(zgui::platform::PlatformError::Backend(
                    format!("No usable DX12 device: {}", errors.join("; ")),
                )));
            }
        }
        let graphics = graphics.borrow();
        let mut renderer = graphics
            .as_ref()
            .expect("device opened above")
            .renderer_for_surface(target, handles, PresentPacing::Display, None)?;
        zgui_render_vector_vello::attach(&mut renderer, target.size);
        eprintln!("Presentation formats: {:?}", renderer.formats());
        Ok(Box::new(renderer) as Box<dyn Renderer>)
    })
}
