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
        ColorParams {
            kind: 6.0,
            channels: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
        ColorParams {
            kind: 6.0,
            channels: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
        ColorParams {
            kind: 6.0,
            channels: [0.83, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    ];
    app().with_size(22.0,98.0).with_renderer(Box::new(factory))
        .with_stylesheet(":root { background-color: #fff; } .sample { width: 22px; height: 14px; min-height: 14px; }")
        .run_on(|handler| {let mut harness=Harness::new(handler);harness.settle(32);Ok(())}, move || {
            zgui::elements::column().child(cases.into_iter().map(|params| zgui::custom::custom(ColorSurface::new(params)).0.class("sample").into_view()).collect::<Vec<_>>())
        }).unwrap();
    let frames = frames.lock().unwrap();
    let image = frames.last().unwrap();
    // At y=9 the checker is visible on both sides of x=10; moving the thumb by three
    // pixels moves its checker UVs while the cells remain fixed in bar coordinates.
    assert_eq!(image.rgba(12, 9), image.rgba(9, 23));
    assert_ne!(image.rgba(9, 9), image.rgba(9, 23));
    for top in [56, 70, 84] {
        let border = image.rgba(11, top + 10)[0];
        assert!(border >= 245, "white outline at row {top}: {border}");
        let near = image.rgba(17, top + 8)[0];
        let far = image.rgba(20, top + 8)[0];
        assert!(
            near > 214 && near + 8 < far && far >= 250,
            "soft outer shadow for every fill at {top}: {near}/{far}"
        );
        assert_eq!(near, image.rgba(4, top + 8)[0], "zero horizontal offset");
    }
    assert_eq!(image.rgba(0, 34), [40, 40, 40, 255]);
    assert_eq!(image.rgba(1, 34), [0, 0, 0, 255]);
    assert_eq!(image.rgba(0, 48), [255, 255, 255, 255]);
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
    // Measure the sRGB contour against the f64 boundary geometry inside the P3 fill.
    // Its red channel has enough contrast here to resolve fractional pixel coverage.
    let params = cases[0];
    let extent = f64::from(params.max_chroma);
    let slice =
        okcolor_core::GamutSlice::new(f64::from(params.channels[2]), extent, Gamut::DisplayP3)
            .unwrap();
    let boundary_x = |y: f64| {
        slice
            .boundary(1.0 - y / f64::from(HEIGHT), Gamut::Srgb)
            .unwrap()
            * f64::from(WIDTH)
            / extent
    };
    let mut partial = 0;
    for y in 29..45 {
        let center_y = f64::from(y) + 0.5;
        let edge = boundary_x(center_y);
        let slope = (boundary_x(center_y + 0.5) - boundary_x(center_y - 0.5)).hypot(1.0);
        for x in edge.floor() as i32 - 3..=edge.ceil() as i32 + 3 {
            let center_x = f64::from(x) + 0.5;
            let source = color(
                params,
                center_x / f64::from(WIDTH),
                center_y / f64::from(HEIGHT),
            );
            let red = source
                .mapped_rgb(Gamut::Srgb, GamutMapping::LocalMinde)
                .unwrap()[0]
                * 255.0;
            let coverage = (1.0 - (center_x - edge).abs() / slope).max(0.0);
            let expected = red * (1.0 - coverage) + 22.0 * coverage;
            let actual = f64::from(image.rgba(x, y)[0]);
            assert!(
                (actual - expected).abs() < 6.0,
                "contour ({x},{y}): {actual} vs {expected}"
            );
            if (0.1..0.9).contains(&coverage) {
                partial += 1;
            }
        }
    }
    assert!(
        partial >= 16,
        "subpixel contour coverage on multiple scanlines"
    );
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
                    expected = expected.map(|v| v * 0.5);
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
