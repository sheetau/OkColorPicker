//! Prove extended color survives retained zgui composition and the scRGB output shader.
//! Readback is test-only; production must keep these textures on the GPU.
use okcolor_core::{Gamut, Oklch};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use zgui::{
    geom::{DevicePx, Point, Rect},
    prelude::*,
    render::{RenderTarget, Renderer},
};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{Gpu, SharedGraphics, gpu::adapter, wgpu};

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
    let source = Rc::new(RefCell::new(None));
    let output = source.clone();
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
            let texture = texture(
                graphics.gpu().unwrap().device(),
                target.size.width as u32,
                16,
            );
            *output.borrow_mut() = Some(texture.clone());
            let renderer = graphics.renderer_supplied(target, vec![texture])?;
            assert_eq!(renderer.formats().scene, wgpu::TextureFormat::Rgba16Float);
            Ok(Box::new(renderer) as Box<dyn Renderer>)
        }))
        .run_on(
            |handler| {
                let mut harness = Harness::new(handler);
                harness.settle(32);
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
    let source = source.borrow();
    let source = source.as_ref().unwrap();
    for scale in [1.0_f32, 2.5] {
        let result = convert(&gpu, source, scale);
        let pixels = read_float(&gpu, &result);
        for (index, color) in colors.iter().enumerate() {
            let got = pixels[8 * 112 + index * 16 + 8];
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
            pixels[8 * 112 + 8][1] < 0.0,
            "P3 red's negative green must survive"
        );
        assert!(
            pixels[8 * 112 + 8][0] > scale,
            "P3 red exceeds the sRGB red coordinate"
        );
    }
}

fn texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
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
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn convert(gpu: &Gpu, source: &wgpu::Texture, scale: f32) -> wgpu::Texture {
    let device = gpu.device();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scRGB presentation"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../src/scrgb.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("scRGB presentation"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("white scale"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue().write_buffer(
        &uniform,
        0,
        &[scale, 0.0, 0.0, 0.0].map(f32::to_le_bytes).concat(),
    );
    let view = source.create_view(&Default::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    let result = texture(device, source.width(), source.height());
    let target = result.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    gpu.queue().submit([encoder.finish()]);
    result
}

fn read_float(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<[f32; 4]> {
    let row = (texture.width() * 8).div_ceil(256) * 256;
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
            let start = (y * row + x * 8) as usize;
            result.push(std::array::from_fn(|c| {
                half::f16::from_le_bytes([mapped[start + c * 2], mapped[start + c * 2 + 1]])
                    .to_f32()
            }));
        }
    }
    drop(mapped);
    buffer.unmap();
    result
}
