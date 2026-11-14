//! Windows presentation policy, kept separate from color calculations and widgets.
use std::{cell::RefCell, sync::Arc};
use zgui::{platform::Surface, render::Renderer, runtime::AppError};
#[cfg(not(target_os = "windows"))]
use zgui_render_wgpu::PresentPacing;
use zgui_render_wgpu::{Gpu, SharedGraphics, gpu::adapter, wgpu};

/// Creates one device lazily, when the first hidden window has a presentation target.
/// Windows recovery retains the same DX12 and DirectComposition policy.
pub fn factory() -> zgui::runtime::RendererFactory {
    factory_with_display(crate::display::DisplayBridge::default())
}

pub fn factory_with_display(
    display: crate::display::DisplayBridge,
) -> zgui::runtime::RendererFactory {
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
        if graphics.borrow().is_none() {
            *graphics.borrow_mut() =
                Some(SharedGraphics::with_gpu(open_gpu().map_err(|error| {
                    AppError::Platform(zgui::platform::PlatformError::Backend(error))
                })?));
        }
        let graphics = graphics.borrow();
        let graphics = graphics.as_ref().expect("device opened above");
        #[cfg(target_os = "windows")]
        {
            let renderer =
                crate::presentation::Presentation::new(graphics, handles, target, display.clone())
                    .map_err(|error| {
                        AppError::Platform(zgui::platform::PlatformError::Backend(error))
                    })?;
            Ok(Box::new(renderer) as Box<dyn Renderer>)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut renderer =
                graphics.renderer_for_surface(target, handles, PresentPacing::Display, None)?;
            zgui_render_vector_vello::attach(&mut renderer, target.size);
            eprintln!("Presentation formats: {:?}", renderer.formats());
            Ok(Box::new(renderer) as Box<dyn Renderer>)
        }
    })
}

/// Use the same compiler and presentation policy on startup and device replacement.
pub(crate) fn open_gpu() -> Result<Arc<Gpu>, String> {
    let mut descriptor = adapter::instance_descriptor(wgpu::Backends::DX12);
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::StaticDxc;
    descriptor.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
    let instance = wgpu::Instance::new(descriptor);
    let mut errors = Vec::new();
    for candidate in adapter::candidates(&instance, wgpu::Backends::DX12) {
        let name = candidate.get_info().name;
        match Gpu::open(instance.clone(), candidate, &[]) {
            Ok(gpu) => {
                eprintln!("Graphics device: {name}; DX12 / DirectComposition / static DXC");
                return Ok(Arc::new(gpu));
            }
            Err(error) => errors.push(error),
        }
    }
    Err(format!("No usable DX12 device: {}", errors.join("; ")))
}
