//! Prove extended color survives retained zgui composition and the scRGB output shader.
//! Readback is test-only; production must keep these textures on the GPU.
use okcolor_core::{Gamut, Oklch};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use zgui::{
    geom::{DevicePx, Point, Rect, Size},
    prelude::*,
    render::{RenderTarget, Renderer},
};
use zgui::{
    platform::SurfaceEvent,
    runtime::{FrameProbe, Window},
};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{Gpu, SharedGraphics, WgpuRenderer, gpu::adapter, wgpu};

struct OutputFrames(Rc<RefCell<Vec<wgpu::Texture>>>);
impl FrameProbe for OutputFrames {
    fn frame_ended(&self, window: &Window) {
        let renderer = window
            .renderer()
            .as_any()
            .unwrap()
            .downcast_ref::<WgpuRenderer>()
            .unwrap();
        let texture = renderer.offscreen_texture().unwrap();
        let mut frames = self.0.borrow_mut();
        if frames
            .last()
            .is_none_or(|last| last.size() != texture.size())
        {
            frames.push(texture.clone());
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
struct Swatch {
    color: [f32; 4],
}
static SWATCH: ShaderEffect<Swatch> = shader! {
    name: "extended-srgb-test", mode: Paint, params: Swatch,
    source: "struct Params { color: vec4<f32> } fn shade(input: ShaderInput, params: Params) -> vec4<f32> { return params.color; }",
};
struct Tile(ShaderHandle<Swatch>);
impl CustomElement for Tile {
    fn layout(&mut self, _: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        CustomMeasured {
            width: 16.0,
            height: 16.0,
            ..Default::default()
        }
    }
    fn paint(&mut self, painter: &mut ScenePainter<'_>) {
        painter.effect(
            Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), painter.size()),
            0.0,
            &self.0,
        );
    }
}

#[test]
#[ignore = "requires a DX12 GPU; verifies FP16 pixels, not monitor reproduction"]
fn fp16_composition_and_scrgb_output_preserve_extended_color_alpha_and_reference_white() {
    let mut descriptor = adapter::instance_descriptor(wgpu::Backends::DX12);
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::StaticDxc;
    let instance = wgpu::Instance::new(descriptor);
    let gpu = adapter::candidates(&instance, wgpu::Backends::DX12)
        .into_iter()
        .find_map(|adapter| Gpu::open(instance.clone(), adapter, &[]).ok())
        .expect("DX12 device");
    let gpu = Arc::new(gpu);
    let graphics = SharedGraphics::with_gpu(gpu.clone());
    let colors = [
        Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::DisplayP3, 1.0, 0.0).unwrap(),
        Oklch::from_rgb([0.0, 1.0, 0.0], Gamut::DisplayP3, 1.0, 0.0).unwrap(),
        Oklch::from_rgb([0.0, 1.0, 0.0], Gamut::Rec2020, 1.0, 0.0).unwrap(),
        Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::DisplayP3, 0.25, 0.0).unwrap(),
        Oklch::from_rgb([0.5, 0.5, 0.5], Gamut::Srgb, 0.5, 0.0).unwrap(),
        Oklch::from_rgb([1.0, 1.0, 1.0], Gamut::Srgb, 1.0, 0.0).unwrap(),
        Oklch::from_rgb([0.0, 0.0, 0.0], Gamut::Srgb, 0.0, 0.0).unwrap(),
    ];
    let frames = Rc::new(RefCell::new(Vec::new()));
    const STYLE: &str = "
        :root { background-color: transparent; }
        row { width: 112px; height: 16px; }
        .tile { width: 16px; height: 16px; min-width: 16px; }
    ";
    app()
        .with_size(16.0 * colors.len() as f32, 16.0)
        .with_transparent(true)
        .with_stylesheet(STYLE)
        .with_renderer(Box::new(move |_, target: RenderTarget| {
            let renderer =
                graphics.renderer_offscreen(target, wgpu::TextureFormat::Rgba16Float, false)?;
            assert_eq!(renderer.formats().scene, wgpu::TextureFormat::Rgba16Float);
            Ok(Box::new(renderer) as Box<dyn Renderer>)
        }))
        .with_probe(Rc::new(OutputFrames(frames.clone())))
        .run_on(
            |handler| {
                let mut harness = Harness::new(handler);
                harness.settle(32);
                for (width, height) in [(128.0, 32.0), (112.0, 16.0)] {
                    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                        DevicePx(width),
                        DevicePx(height),
                    )));
                    harness.settle(32);
                }
                Ok(())
            },
            move || {
                zgui::elements::row().child(
                    colors
                        .into_iter()
                        .map(|color| {
                            let alpha = color.components()[3] as f32;
                            let rgb = color.encoded_rgb(Gamut::Srgb).map(|v| v as f32 * alpha);
                            let shader = SWATCH.register();
                            shader.set_params(Swatch {
                                color: [rgb[0], rgb[1], rgb[2], alpha],
                            });
                            zgui::custom::custom(Tile(shader))
                                .0
                                .class("tile")
                                .into_view()
                        })
                        .collect::<Vec<_>>(),
                )
            },
        )
        .unwrap();
    let frames = frames.borrow();
    assert_eq!(
        frames.len(),
        3,
        "Growth and shrink must replace the offscreen output"
    );
    let mut pass = okcolor_app::output_pass::OutputPass::new(
        &gpu,
        &frames[0],
        wgpu::TextureFormat::Rgba16Float,
    );
    for source in frames.iter() {
        pass.bind_source(&gpu, source);
        for scale in [1.0_f32, 2.5] {
            let result = convert(&gpu, &pass, source, scale, wgpu::TextureFormat::Rgba16Float);
            let pixels = read_float(&gpu, &result);
            for (index, color) in colors.iter().enumerate() {
                let got = pixels[8 * source.width() as usize + index * 16 + 8];
                let alpha = color.components()[3];
                let expected = color
                    .linear_rgb(Gamut::Srgb)
                    .map(|v| v * alpha * f64::from(scale));
                for channel in 0..3 {
                    assert!(
                        (f64::from(got[channel]) - expected[channel]).abs() < 0.006,
                        "sample {index}, scale {scale}, channel {channel}: {got:?}, expected {expected:?}"
                    );
                }
                assert!((f64::from(got[3]) - alpha).abs() < 0.001);
            }
            assert!(
                pixels[8 * source.width() as usize + 8][1] < 0.0,
                "P3 red's negative green must survive"
            );
            assert!(
                pixels[8 * source.width() as usize + 8][0] > scale,
                "P3 red exceeds the sRGB red coordinate"
            );
        }
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
        ] {
            let fallback = okcolor_app::output_pass::OutputPass::new(&gpu, source, format);
            let result = convert(&gpu, &fallback, source, 2.5, format);
            let pixels = read_float(&gpu, &result);
            for (index, color) in colors.iter().enumerate() {
                let got = pixels[8 * source.width() as usize + index * 16 + 8];
                let alpha = color.components()[3];
                let expected = color
                    .encoded_rgb(Gamut::Srgb)
                    .map(|v| (v * alpha).clamp(0.0, 1.0));
                for channel in 0..3 {
                    assert!(
                        (f64::from(got[channel]) - expected[channel]).abs() < 0.006,
                        "SDR passthrough changed gamma or white: {got:?} vs {expected:?}"
                    );
                }
                assert!((f64::from(got[3]) - alpha).abs() < 0.006);
            }
        }
    }
}

fn texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("wide-color-test"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn convert(
    gpu: &Gpu,
    pass: &okcolor_app::output_pass::OutputPass,
    source: &wgpu::Texture,
    scale: f32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    let output = texture(gpu.device(), source.width(), source.height(), format);
    let view = output.create_view(&Default::default());
    let mut encoder = gpu.device().create_command_encoder(&Default::default());
    pass.encode(gpu, &mut encoder, &view, scale);
    gpu.queue().submit([encoder.finish()]);
    output
}

fn read_float(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<[f32; 4]> {
    let float = texture.format() == wgpu::TextureFormat::Rgba16Float;
    let stride = if float { 8 } else { 4 };
    let row = (texture.width() * stride).div_ceil(256) * 256;
    let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device().create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    gpu.queue().submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    gpu.wait();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    let mut result = Vec::new();
    for y in 0..texture.height() {
        for x in 0..texture.width() {
            let start = (y * row + x * stride) as usize;
            result.push(std::array::from_fn(|c| {
                if float {
                    half::f16::from_le_bytes([mapped[start + c * 2], mapped[start + c * 2 + 1]])
                        .to_f32()
                } else {
                    let channel = if texture.format() == wgpu::TextureFormat::Bgra8Unorm {
                        [2, 1, 0, 3][c]
                    } else {
                        c
                    };
                    f32::from(mapped[start + channel]) / 255.0
                }
            }));
        }
    }
    drop(mapped);
    buffer.unmap();
    result
}
