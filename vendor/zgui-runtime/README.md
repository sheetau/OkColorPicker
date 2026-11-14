# Pinned zgui runtime

Source: [zortax/zgui](https://github.com/zortax/zgui), revision
`80baa3c7cb0fbbe1c058f10059e076e41c0a1b9a`.
This package contains `crates/zgui-runtime/src`, LICENSE and NOTICE.
The standalone manifest resolves runtime dependencies to the existing lockfile
versions and the same zgui revision. Upstream integration tests are not vendored.

The only functional change is in `src/window/frame.rs`: `FrameOutcome::Recovered`
invalidates caches with `Window::forget_caches()` rather than only re-noting paint
chunks. The new GPU has no atlas textures or retained geometry; replaying old
records cannot restore these resources. Cache invalidation keeps the mounted
view, focus, editing state and application model, then rebuilds paint and uploads.

`native_recovery` destroys only its own logical DX12 device twice. It requires
actual native presentations and compares the mounted Picker pixels before and
after recovery, including glyphs and custom paint, while retaining an active drag
and its committed history. It does not reset the system display driver.

When updating zgui, refresh the pinned source, reapply this change if still needed,
and run all product checks, including the explicit GPU/native tests.
