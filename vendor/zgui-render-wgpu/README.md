# Pinned zgui renderer

Source: [zortax/zgui](https://github.com/zortax/zgui), revision
`80baa3c7cb0fbbe1c058f10059e076e41c0a1b9a`.
This package contains `crates/zgui-render-wgpu/src`, `build.rs`, LICENSE and NOTICE.
The standalone manifest resolves workspace dependencies to their existing lockfile
versions and the same zgui revision. Upstream integration tests are not vendored.

The local renderer adds `WgpuRenderer::offscreen_texture()` in
`src/renderer/mod.rs`. It borrows the latest offscreen output on the GPU; it does
not read pixels to the CPU or change composition. Its contents remain premultiplied,
gamma-encoded values even for FP16. The caller reacquires and rebinds the texture
after resize. Surface and supplied renderers return `None`.

OkColorPicker uses this texture for its final scRGB pass, preserving the retained
renderer, atlas and resize behavior. The application owns the native swap chain.
`wide_pipeline` checks output pixels after growth/shrink, and `native_caption`
requires actual native presentation at maximized and restored extents.

On DX12, the device requests at most 65,536 non-sampler bindings. This limits
the descriptor heap allocated at device creation for the picker.

When updating zgui, refresh the source at the pinned revision, reapply this
accessor and the DX12 binding limit, then run all product checks, including the explicit GPU/native tests.

## License files

LICENSE and NOTICE contain the complete upstream texts. The upstream repository
stores relative symlinks in this package; their contents are expanded here so a
Windows checkout and a source archive retain the notices without symlink support.
