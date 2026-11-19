//! Mounted picker pixels under simulated display policies; no monitor claim is made here.
use okcolor_app::{
    display::{DisplayBridge, DisplayMode, DisplayStatus},
    model::PickerModel,
    output_pass::OutputPass,
    persistence::SavedState,
    surface::{ColorParams, SurfaceKind},
    ui::{PickerProps, PickerRefs, STYLE},
};
use okcolor_core::{Gamut, GamutMapping, Oklch};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use zgui::{
    prelude::*,
    render::{RenderTarget, Renderer},
    runtime::{FrameProbe, Window},
};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{Gpu, SharedGraphics, WgpuRenderer, gpu::adapter, wgpu};
#[path = "support/float_pixels.rs"]
mod float_pixels;

struct Frame(Rc<RefCell<Option<wgpu::Texture>>>);
impl FrameProbe for Frame {
    fn frame_ended(&self, window: &Window) {
        let renderer = window
            .renderer()
            .as_any()
            .unwrap()
            .downcast_ref::<WgpuRenderer>()
            .unwrap();
        *self.0.borrow_mut() = Some(renderer.offscreen_texture().unwrap().clone());
    }
}

fn final_pixels(gpu: &Gpu, source: &wgpu::Texture, white: f32) -> Vec<[f32; 4]> {
    let output = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("picker scRGB readback"),
        size: source.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let pass = OutputPass::new(gpu, source, output.format());
    let mut encoder = gpu.device().create_command_encoder(&Default::default());
    pass.encode(
        gpu,
        &mut encoder,
        &output.create_view(&Default::default()),
        white,
    );
    gpu.queue().submit([encoder.finish()]);
    float_pixels::read_float(gpu, &output)
}

fn preview(color: Oklch, wide: bool, mapping: GamutMapping) -> [f64; 3] {
    if wide {
        color.encoded_rgb(Gamut::Srgb)
    } else {
        color.mapped_rgb(Gamut::Srgb, mapping).unwrap()
    }
}

fn expect_pixel(pixels: &[[f32; 4]], x: i32, y: i32, rgb: [f64; 3], white: f32, label: &str) {
    let actual = pixels[(y * 330 + x) as usize];
    for channel in 0..3 {
        let v = rgb[channel];
        let linear = if v.abs() <= 0.04045 {
            v / 12.92
        } else {
            v.signum() * ((v.abs() + 0.055) / 1.055).powf(2.4)
        };
        let expected = linear * f64::from(white);
        assert!(
            (f64::from(actual[channel]) - expected).abs() < 0.018,
            "{label} at {x},{y}: {actual:?} expected {expected} in channel {channel}"
        );
    }
    assert!(
        (actual[3] - 1.0).abs() < 0.001,
        "The checker-composited picker must stay opaque"
    );
}

fn color_at(params: ColorParams, u: f64, v: f64) -> Oklch {
    let [mut l, mut c, mut h, _] = params.channels.map(f64::from);
    match params.kind as u8 {
        0 => {
            l = 1.0 - v;
            c = u * f64::from(params.max_chroma);
        }
        1 => l = u,
        2 => c = u * f64::from(params.max_chroma),
        3 => h = u * 360.0,
        _ => {}
    }
    Oklch::new(l, c, h, 1.0).unwrap()
}

#[test]
#[ignore = "requires a DX12 GPU; simulates display states without changing Windows settings"]
fn mounted_picker_preserves_wide_color_and_repaints_every_sample_on_display_transitions() {
    let mut descriptor = adapter::instance_descriptor(wgpu::Backends::DX12);
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::StaticDxc;
    let instance = wgpu::Instance::new(descriptor);
    let gpu = Arc::new(
        adapter::candidates(&instance, wgpu::Backends::DX12)
            .into_iter()
            .find_map(|adapter| Gpu::open(instance.clone(), adapter, &[]).ok())
            .expect("DX12 device"),
    );
    let graphics = SharedGraphics::with_gpu(gpu.clone());
    let output = Rc::new(RefCell::new(None));
    let state = Rc::new(Cell::new(None));
    let mounted = state.clone();
    let bridge = DisplayBridge::default();
    let bind = bridge.clone();
    let red = Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::DisplayP3, 1.0, 0.0).unwrap();
    let green = Oklch::from_rgb([0.0, 1.0, 0.0], Gamut::Rec2020, 1.0, 0.0).unwrap();
    app()
        .with_size(330.0, 560.0)
        .with_transparent(true)
        .with_stylesheet(STYLE)
        .with_renderer(Box::new(move |_, target: RenderTarget| {
            let mut renderer =
                graphics.renderer_offscreen(target, wgpu::TextureFormat::Rgba16Float, false)?;
            zgui_render_vector_vello::attach(&mut renderer, target.size);
            Ok(Box::new(renderer) as Box<dyn Renderer>)
        }))
        .with_probe(Rc::new(Frame(output.clone())))
        .run_on(
            |handler| {
                let mut harness = Harness::new(handler);
                harness.settle(64);
                let (model, refs): (PickerModel, PickerRefs) = state.get().unwrap();
                let mut checked = 0;
                for status in [
                    DisplayStatus {
                        mode: DisplayMode::Sdr,
                        wide_output: false,
                    },
                    DisplayStatus {
                        mode: DisplayMode::WideColor,
                        wide_output: true,
                    },
                    DisplayStatus {
                        mode: DisplayMode::Hdr,
                        wide_output: true,
                    },
                    DisplayStatus {
                        mode: DisplayMode::Hdr,
                        wide_output: false,
                    },
                    DisplayStatus::default(),
                ] {
                    bridge.publish(status);
                    for mapping in [GamutMapping::LocalMinde, GamutMapping::Clip] {
                        model.mapping.set(mapping);
                        harness.settle(64);
                        assert_eq!(model.current(), red);
                        assert!(!model.can_undo.get_untracked());
                        let white = if status.mode == DisplayMode::Hdr && status.wide_output {
                            2.5
                        } else {
                            1.0
                        };
                        let source = output.borrow().as_ref().unwrap().clone();
                        let pixels = final_pixels(&gpu, &source, white);
                        for (node, color, offset) in [
                            (refs.swatches[1], green, 30.0),
                            (refs.swatches[0], red, 14.0),
                            (refs.recents, green, 11.0),
                        ] {
                            let bounds = node.window_bounds().unwrap();
                            expect_pixel(
                                &pixels,
                                (bounds.origin.x.0 + offset) as i32,
                                (bounds.origin.y.0 + offset - 4.0) as i32,
                                if node != refs.recents
                                    && !status.wide_output
                                    && !color.in_gamut(Gamut::Srgb)
                                {
                                    [0.0; 3]
                                } else {
                                    preview(color, status.wide_output, mapping)
                                },
                                white,
                                "swatch/recent",
                            );
                            checked += 1;
                        }
                        for (node, kind) in [
                            (refs.slice, SurfaceKind::Slice),
                            (refs.bars[2], SurfaceKind::Hue),
                        ] {
                            let bounds = node.window_bounds().unwrap();
                            let _zone = zgui::reactive::enter_non_reactive_zone();
                            let params = kind.params(model);
                            for fraction in [0.13, 0.37, 0.61, 0.83] {
                                let x = (bounds.origin.x.0 + bounds.size.width.0 * fraction).floor()
                                    as i32;
                                let y = (bounds.origin.y.0 + bounds.size.height.0 * 0.73).floor()
                                    as i32;
                                let u = (f64::from(x) + 0.5 - f64::from(bounds.origin.x.0))
                                    / f64::from(bounds.size.width.0);
                                let v = (f64::from(y) + 0.5 - f64::from(bounds.origin.y.0))
                                    / f64::from(bounds.size.height.0);
                                let marker = kind.marker(model);
                                if (u - marker[0]).abs() * f64::from(bounds.size.width.0) < 10.0
                                    && (v - marker[1]).abs() * f64::from(bounds.size.height.0)
                                        < 10.0
                                {
                                    continue;
                                }
                                let color = color_at(params, u, v);
                                if kind != SurfaceKind::Alpha
                                    && [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020]
                                        .into_iter()
                                        .any(|g| {
                                            let inside = color.in_gamut(g);
                                            [
                                                (-2.0 / f64::from(bounds.size.width.0), 0.0),
                                                (2.0 / f64::from(bounds.size.width.0), 0.0),
                                                (0.0, -2.0 / f64::from(bounds.size.height.0)),
                                                (0.0, 2.0 / f64::from(bounds.size.height.0)),
                                            ]
                                            .into_iter()
                                            .any(
                                                |(dx, dy)| {
                                                    color_at(params, u + dx, v + dy).in_gamut(g)
                                                        != inside
                                                },
                                            )
                                        })
                                {
                                    continue;
                                }
                                let mut rgb = if kind == SurfaceKind::Slice
                                    && !color.in_gamut(Gamut::Rec2020)
                                {
                                    [44.0 / 255.0, 42.0 / 255.0, 48.0 / 255.0]
                                } else {
                                    preview(color, status.wide_output, mapping)
                                };
                                if kind == SurfaceKind::Alpha {
                                    let lx = (x as f32 + 0.5 - bounds.origin.x.0) / 5.0;
                                    let ly = (y as f32 + 0.5 - bounds.origin.y.0) / 5.0;
                                    let backdrop = if (lx as u32 + ly as u32) % 2 == 1 {
                                        0.23
                                    } else {
                                        0.13
                                    };
                                    rgb = rgb.map(|c| backdrop * (1.0 - u) + c * u);
                                }
                                if kind == SurfaceKind::Hue && !color.in_gamut(Gamut::Rec2020) {
                                    let normal = (f64::from(x + y + 1)
                                        - f64::from(bounds.origin.x.0 + bounds.origin.y.0))
                                        * std::f64::consts::FRAC_1_SQRT_2;
                                    let distance = (normal - 2.0 * (normal / 2.0).round()).abs();
                                    rgb = rgb.map(|v| v * distance.min(1.0));
                                }
                                expect_pixel(&pixels, x, y, rgb, white, "surface");
                                checked += 1;
                            }
                        }
                    }
                }
                assert!(checked >= 85, "Too few mounted samples checked: {checked}");
                eprintln!("Verified {checked} mounted color samples across display policies");
                bridge.publish(DisplayStatus {
                    mode: DisplayMode::WideColor,
                    wide_output: true,
                });
                for alpha in [0.25, 0.0] {
                    let [l, c, h, _] = red.components();
                    let translucent = Oklch::new(l, c, h, alpha).unwrap();
                    model.select(translucent);
                    harness.settle(64);
                    let pixels = final_pixels(&gpu, output.borrow().as_ref().unwrap(), 1.0);
                    let bounds = refs.swatches[0].window_bounds().unwrap();
                    let rgb = red
                        .encoded_rgb(Gamut::Srgb)
                        .map(|v| 0.13 * (1.0 - alpha) + v * alpha);
                    expect_pixel(
                        &pixels,
                        (bounds.origin.x.0 + 18.0) as i32,
                        (bounds.origin.y.0 + 18.0) as i32,
                        rgb,
                        1.0,
                        "alpha swatch",
                    );
                    assert_eq!(model.current(), translucent);
                }
                // Extreme finite input remains exact in the model and cannot poison FP16 output.
                bridge.publish(DisplayStatus {
                    mode: DisplayMode::WideColor,
                    wide_output: true,
                });
                let extreme = Oklch::new(0.5, 1e100, 21.0, 0.5).unwrap();
                model.select(extreme);
                harness.settle(64);
                let pixels = final_pixels(&gpu, output.borrow().as_ref().unwrap(), 1.0);
                assert!(pixels.iter().flatten().all(|v| v.is_finite()));
                assert_eq!(model.current(), extreme);
                Ok(())
            },
            move || {
                let model = PickerModel::restore(SavedState {
                    main: red,
                    sub: green,
                    recent: vec![green, red],
                    gamut: Gamut::Rec2020,
                    ..Default::default()
                });
                bind.bind(model.display);
                let refs = PickerRefs::default();
                mounted.set(Some((model, refs)));
                view! { Picker(model=model,refs=refs) }
            },
        )
        .unwrap();
}
