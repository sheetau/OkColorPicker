# Pinned zgui platform backend

Source: [zortax/zgui](https://github.com/zortax/zgui), revision
`80baa3c7cb0fbbe1c058f10059e076e41c0a1b9a`.
Only `crates/zgui-platform-winit/src` is vendored. LICENSE and NOTICE are copied
from that revision; other zgui crates remain Git dependencies at the same revision.

The sole source modification is in `src/surface/attributes.rs`: the opt-in
`direct-composition` feature calls `with_no_redirection_bitmap(true)` on Windows
before window creation. This gives winit ownership of the flag, preserves it
through style updates, and bypasses its legacy transparent-window blur path.
Other platforms and builds without the feature retain upstream behavior.

The standalone Cargo manifest expands workspace inheritance, pins external
dependencies to the application's resolved versions, and adds the feature.
Upstream integration tests are not vendored. The application's
`tests/native_window.rs` checks the modified backend against real hidden HWNDs.

When updating zgui, copy this package's source from the new pinned revision,
reapply the marked change, update its manifest, and run the application checks.
Remove the patch when upstream offers an equivalent creation-time option.
