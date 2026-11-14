# Pinned zgui shader vocabulary

Source: [zortax/zgui](https://github.com/zortax/zgui), revision
`80baa3c7cb0fbbe1c058f10059e076e41c0a1b9a`.
This package contains `crates/zgui-wgsl/src` plus the upstream LICENSE and NOTICE.
`LICENSE-MIT` retains the [Windows Terminal license](https://github.com/microsoft/terminal/blob/main/LICENSE)
for the derived text-rendering code identified in NOTICE.
The standalone manifest replaces workspace inheritance and pins `zgui-scene`
to the same revision. Upstream integration tests are not vendored.

The sole functional source modification is in `src/wgsl/shaded_paint.wgsl`:
custom paint preserves extended RGB instead of clamping all channels to 0–1.
Alpha is still clamped. Negative and above-one channels represent valid
wide-gamut colors in extended sRGB; FP16 targets must retain them until output.
Normalized render targets continue to constrain stored values.
The extra blank line at the end of `shadow.wgsl` is removed for whitespace checks.

This applies to the shared shader vocabulary used by both the procedural macro
and renderer. Filter shaders retain upstream behavior; this patch does not
establish wide-gamut correctness for arbitrary CSS filters or other paint APIs.
`okcolor-app/tests/wide_pipeline.rs` checks the custom-paint path through retained
FP16 composition and the final scRGB conversion against the independent f64 core.

When updating zgui, refresh the source at the new pinned revision and reapply
this marked change. Run both the ordinary SDR tests and the FP16 pipeline test.
