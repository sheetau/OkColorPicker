//! Compare actual GPU shader pixels with the independent f64 color core.
use okcolor_app::surface::{ColorParams, ColorSurface};
use okcolor_core::{Gamut, GamutMapping, Oklch};
use std::sync::{Arc, Mutex};
use zgui::{
    platform::Surface,
    prelude::*,
    render::{RenderTarget, Renderer},
};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{Builder, wgpu};
mod support;
use support::Capture;

const WIDTH: i32 = 256;
const HEIGHT: i32 = 64;

#[test]
#[ignore = "requires a graphics device; run with --ignored"]
fn marker_checker_has_a_fixed_origin_and_dark_samples_have_inner_borders() {
    let frames = Arc::new(Mutex::new(Vec::new()));
    let capture = frames.clone();
    let factory = move |_: &Arc<dyn Surface>, target: RenderTarget| {
        let renderer = Builder::new()
            .offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false)
            .map_err(zgui::runtime::AppError::GpuUnavailable)?;
        Ok(Box::new(Capture {
            renderer,
            frames: capture.clone(),
        }) as Box<dyn Renderer>)
    };
    let cases = [
        ColorParams {
            kind: 6.0,
            checker_offset: [0.0, 14.0],
            ..Default::default()
        },
        ColorParams {
            kind: 6.0,
            checker_offset: [3.0, 14.0],
            ..Default::default()
        },
        ColorParams {
            kind: 5.0,
            channels: [0.0, 0.0, 0.0, 1.0],
            outline_dark: 1.0,
            ..Default::default()
        },
        ColorParams {
            kind: 5.0,
            channels: [1.0, 0.0, 0.0, 1.0],
            outline_dark: 1.0,
            ..Default::default()
        },
    ];
    app().with_size(16.0,64.0).with_renderer(Box::new(factory))
        .with_stylesheet(":root { background-color: #000; } .sample { width: 16px; height: 16px; min-height: 16px; }")
        .run_on(|handler| {let mut harness=Harness::new(handler);harness.settle(32);Ok(())}, move || {
            zgui::elements::column().child(cases.into_iter().map(|params| zgui::custom::custom(ColorSurface::new(params)).0.class("sample").into_view()).collect::<Vec<_>>())
        }).unwrap();
    let frames = frames.lock().unwrap();
    let image = frames.last().unwrap();
    assert_eq!(
        image.rgba(8, 8),
        image.rgba(5, 24),
        "a three-pixel move must leave checker cells fixed in bar coordinates"
    );
    assert_ne!(
        image.rgba(7, 8),
        image.rgba(7, 24),
        "checker cells must not travel with the marker"
    );
    assert_eq!(image.rgba(0, 40), [40, 40, 40, 255]);
    assert_eq!(image.rgba(1, 40), [0, 0, 0, 255]);
    assert_eq!(image.rgba(8, 40), [0, 0, 0, 255]);
    assert_eq!(
        image.rgba(0, 56),
        [255, 255, 255, 255],
        "bright samples must not gain a border"
    );
}

fn color(params: ColorParams, x: f64, y: f64) -> Oklch {
    let [mut l, mut c, mut h, _] = params.channels.map(f64::from);
    match params.kind as u8 {
        0 if params.fixed_axis == 1.0 => {
            h = 360.0 * x;
            c = (1.0 - y) * f64::from(params.max_chroma);
        }
        0 if params.fixed_axis == 2.0 => {
            h = 360.0 * x;
            l = 1.0 - y;
        }
        0 => {
            l = 1.0 - y;
            c = x * f64::from(params.max_chroma);
        }
        1 => l = x,
        2 => c = x * f64::from(params.max_chroma),
        3 => h = x * 360.0,
        _ => {}
    }
    Oklch::new(l, c, h, 1.0).unwrap()
}

#[test]
#[ignore = "requires a graphics device; run with --ignored"]
fn gpu_surfaces_match_f64_mapping_and_working_gamut_mask() {
    let mut cases: Vec<_> = [30.0, 145.0, 270.0]
        .into_iter()
        .flat_map(|h| {
            [0.0, 1.0].into_iter().flat_map(move |mapping| {
                (0..5).map(move |kind| ColorParams {
                    channels: [0.7, 0.3, h, 1.0],
                    gamut: 1.0,
                    kind: kind as f32,
                    mapping,
                    max_chroma: 0.4,
                    ..Default::default()
                })
            })
        })
        .collect();
    for fixed_axis in [1.0, 2.0] {
        cases.push(ColorParams {
            fixed_axis,
            ..cases[0]
        });
    }
    let frames = Arc::new(Mutex::new(Vec::new()));
    let capture = frames.clone();
    let count = cases.len();
    let draw_cases = cases.clone();
    let factory = move |_: &Arc<dyn Surface>, target: RenderTarget| {
        let renderer = Builder::new()
            .offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false)
            .map_err(zgui::runtime::AppError::GpuUnavailable)?;
        Ok(Box::new(Capture {
            renderer,
            frames: capture.clone(),
        }) as Box<dyn Renderer>)
    };
    app().with_size(WIDTH as f32, HEIGHT as f32 * count as f32).with_renderer(Box::new(factory))
        .with_stylesheet(".surfaces { width: 256px; } .surface { width: 256px; height: 64px; min-height: 64px; flex-shrink: 0; }")
        .run_on(|handler| {let mut harness = Harness::new(handler); harness.settle(32); Ok(())}, move || {
            zgui::elements::column().class("surfaces").child(draw_cases.iter().copied().map(|params| {
                zgui::custom::custom(ColorSurface::new(params)).0.class("surface").into_view()
            }).collect::<Vec<_>>())
        }).expect("draw shader samples");
    let frames = frames.lock().unwrap();
    let image = frames.last().expect("shader frame");
    let mut samples = 0;
    for (row, params) in cases.into_iter().enumerate() {
        for y in [0, 7, 18, 19, 20, 23, 41, 57, HEIGHT - 1] {
            for x in (7..WIDTH - 7).step_by(13).chain([191, 192, 193]) {
                let u = (f64::from(x) + 0.5) / f64::from(WIDTH);
                let v = (f64::from(y) + 0.5) / f64::from(HEIGHT);
                // Boundary ink is intentionally decorative. Compare color/mask pixels away
                // from a crossing; a two-pixel neighborhood also avoids f32 edge ambiguity.
                if params.kind < 4.0
                    && [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020]
                        .into_iter()
                        .any(|g| {
                            let inside = color(params, u, v).in_gamut(g);
                            [
                                (-2.0 / f64::from(WIDTH), 0.0),
                                (2.0 / f64::from(WIDTH), 0.0),
                                (0.0, -2.0 / f64::from(HEIGHT)),
                                (0.0, 2.0 / f64::from(HEIGHT)),
                            ]
                            .into_iter()
                            .any(|(dx, dy)| {
                                color(params, (u + dx).clamp(0.0, 1.0), (v + dy).clamp(0.0, 1.0))
                                    .in_gamut(g)
                                    != inside
                            })
                        })
                {
                    continue;
                }
                let source = color(params, u, v);
                let method = if params.mapping == 0.0 {
                    GamutMapping::LocalMinde
                } else {
                    GamutMapping::Clip
                };
                let mut expected = if params.kind == 0.0 && !source.in_gamut(Gamut::DisplayP3) {
                    [9.0 / 255.0; 3]
                } else {
                    source.mapped_rgb(Gamut::Srgb, method).unwrap()
                };
                if (1.0..=3.0).contains(&params.kind) && !source.in_gamut(Gamut::DisplayP3) {
                    let normal = f64::from(x + y + 1) * std::f64::consts::FRAC_1_SQRT_2;
                    let distance = (normal - 2.0 * (normal / 2.0).round()).abs();
                    expected = expected.map(|v| v * distance.min(1.0));
                }
                if params.kind == 0.0 {
                    let col = (f64::from(if params.fixed_axis == 0.0 {
                        params.channels[1] / params.max_chroma
                    } else {
                        params.channels[2] / 360.0
                    }) * f64::from(WIDTH))
                    .round() as i32;
                    let row = ((1.0
                        - f64::from(if params.fixed_axis == 1.0 {
                            params.channels[1] / params.max_chroma
                        } else {
                            params.channels[0]
                        }))
                        * f64::from(HEIGHT))
                    .round() as i32;
                    let paper = [44.0 / 255.0, 42.0 / 255.0, 48.0 / 255.0];
                    for on_axis in [y == row, x == col] {
                        if on_axis {
                            expected = std::array::from_fn(|i| {
                                expected[i] * 0.6 + (expected[i] - paper[i]).abs() * 0.4
                            });
                        }
                    }
                }
                if params.kind == 4.0 {
                    let backdrop = if (x / 5 + y / 5) % 2 == 1 { 0.23 } else { 0.13 };
                    expected = expected.map(|channel| backdrop * (1.0 - u) + channel * u);
                }
                let actual = image.rgba(x, row as i32 * HEIGHT + y);
                for channel in 0..3 {
                    let expected = (expected[channel] * 255.0).round() as u8;
                    assert!(
                        actual[channel].abs_diff(expected) <= 2,
                        "row {row}, ({x},{y}), channel {channel}: GPU {actual:?}, CPU {expected}"
                    );
                }
                assert_eq!(actual[3], 255);
                samples += 1;
            }
        }
    }
    assert!(samples > 1500, "too few color pixels checked: {samples}");
}
