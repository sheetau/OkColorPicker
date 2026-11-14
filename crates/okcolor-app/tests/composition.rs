//! GPU readback contract for the application's opaque scene in an alpha surface.
//! The recording adapter follows zgui's Apache-2.0 translucent test pattern.
//! Copyright 2026 Leonard Seibold. Adapted for OkColorPicker's scene contract.
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

/// Run explicitly on a machine with a graphics device; absence is a failure, not a pass.
#[test]
#[ignore = "requires a graphics device; run with --ignored"]
fn opaque_scene_and_translucent_pixels_have_the_expected_alpha() {
    let frames = Arc::new(Mutex::new(Vec::new()));
    let capture = frames.clone();
    let factory = move |_: &Arc<dyn Surface>, target: RenderTarget| {
        assert!(!target.opaque, "window must request an alpha surface");
        let renderer = Builder::new()
            .offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false)
            .map_err(zgui::runtime::AppError::GpuUnavailable)?;
        Ok(Box::new(Capture {
            renderer,
            frames: capture.clone(),
        }) as Box<dyn Renderer>)
    };
    app()
        .with_size(120.0, 40.0)
        .with_transparent(true)
        .with_renderer(Box::new(factory))
        .with_stylesheet(css!(
            "
            .row { display: flex; flex-direction: row; width: 120px; height: 40px; }
            .tile { width: 40px; height: 40px; flex-shrink: 0; }
            .opaque { background-color: rgb(24, 24, 27); }
            .alpha { background-color: rgba(255, 0, 0, 0.5); }
        "
        ))
        .run_on(
            |handler| {
                let mut harness = Harness::new(handler);
                harness.settle(32);
                assert_eq!(
                    harness.run_for(
                        std::time::Duration::from_secs(5),
                        std::time::Duration::from_millis(100)
                    ),
                    0,
                    "static scene must settle"
                );
                Ok(())
            },
            || {
                view! {
                    row(class = "row") {
                        box(class = "tile opaque") {}
                        box(class = "tile alpha") {}
                        box(class = "tile") {}
                    }
                }
            },
        )
        .expect("render test scene");
    let frames = frames.lock().unwrap();
    let pixels = frames.last().expect("at least one rendered frame");
    assert_eq!(pixels.rgba(20, 20), [24, 24, 27, 255]);
    let red = pixels.rgba(60, 20);
    assert!(
        red[0].abs_diff(128) <= 1 && red[3].abs_diff(128) <= 1,
        "{red:?}"
    );
    assert_eq!([red[1], red[2]], [0, 0]);
    assert_eq!(pixels.rgba(100, 20), [0, 0, 0, 0]);
}
