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

fn color(params: ColorParams, x: f64, y: f64) -> Oklch {
    let [mut l, mut c, mut h, _] = params.channels.map(f64::from);
    match params.kind as u8 {
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
    let cases: Vec<_> = [30.0, 145.0, 270.0]
        .into_iter()
        .flat_map(|h| {
            [0.0, 1.0].into_iter().flat_map(move |mapping| {
                (0..5).map(move |kind| ColorParams {
                    channels: [0.7, 0.3, h, 1.0],
                    gamut: 1.0,
                    kind: kind as f32,
                    mapping,
                    max_chroma: 0.4,
                })
            })
        })
        .collect();
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
        for y in [7, 23, 41, 57] {
            for x in (7..WIDTH - 7).step_by(13) {
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
                            .any(|(dx, dy)| color(params, u + dx, v + dy).in_gamut(g) != inside)
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
