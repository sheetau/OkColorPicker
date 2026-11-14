# Development

## Prerequisites

- Windows with the MSVC C++ build tools and Windows SDK.
- Rust 1.98.0 (MSVC), rustfmt and Clippy. `rust-toolchain.toml` pins the toolchain
  for rustup installations; standalone installations must supply the same version.
- Python 3.11 or later.
- Git and a browser.

## Verify the environment

```powershell
python scripts/check_environment.py
```

The environment check compiles and runs a temporary Rust binary to verify the
native linker. It creates no product source.

## Build and run

```powershell
cargo run -p okcolor-app --locked
```

The executable uses DX12, DirectComposition, `WS_EX_NOREDIRECTIONBITMAP`,
premultiplied surface alpha and an opaque CSS background. A statically linked DXC
avoids loading unrelated `dxcompiler.dll` files from PATH. It logs the
selected adapter and actual surface formats; set `OKCOLOR_TRACE_FRAMES` to log
frame counts. It requires a working
DX12 device and does not yet recover from device loss.

The Windows presentation path composes into retained FP16 textures, then converts
premultiplied gamma-encoded values to linear scRGB on the GPU. It prefers an FP16
DirectComposition swap chain when the surface supports it and the display's
reference white is known. Otherwise it uses an unencoded 8-bit SDR surface and
copies the original gamma-encoded values. Alpha stays premultiplied in both paths.
Resize replaces the offscreen output and its binding without replacing the UI
renderer or its atlas. Surface acquisition failures preserve the submitted frame
for retry; device-loss recovery is still required.

The slice, four channel bars, current/previous samples and recent samples all use
custom GPU paint. They retain extended RGB when Windows reports active WCG or HDR,
reference white is known, and the actual presentation format is FP16. Otherwise
they use the selected sRGB mapping or clipping policy. The status names the
active preview path; the sRGB output policy remains available for bounded text
formats. Physical panel coverage and reproduction still require native validation.

Display status is runtime-only and never changes the source color, saved state or
undo history. When its policy changes, the renderer defers the old scene with its
damage intact, allowing reactive paint effects to update before presentation.
Samples composite alpha over a checkerboard; the opaque application scene stays
opaque. The underlying numeric alpha remains unchanged.

Read-only discovery uses the window's principal display, active color mode, bit
depth and reference white. Support flags alone do not enable wide output. Native
window-position, DPI, display-change and setting-change messages invalidate the
cached discovery and request a frame. On Windows 11 build 22621 and later, a cached
`DisplayInformation` also subscribes to `AdvancedColorInfoChanged`, covering color
settings independently of window messages. Callbacks only invalidate the cache;
the following draw queries it outside the callback. Older systems or subscription
failures retain the window-message path and log the limitation.

The native entry point owns a UI-thread WinRT dispatcher scope outside the app's
event loop. Subscriptions are revoked before releasing their window; an owned
queue drains and shuts down before COM uninitialization. Existing host queues are
borrowed and remain running. Custom native hosts must likewise retain
`windows_dispatcher::DispatcherRuntime` around their event loop to enable these
events. The interop interface follows the Windows SDK 10.0.26100.0 ABI and
[Microsoft's desktop DisplayInformation contract](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.display.interop/nf-windows-graphics-display-interop-idisplayinformationstaticsinterop-getforwindow).
Actual cross-monitor/HDR transitions, including reference-white slider changes,
still need native validation; successful subscription does not establish delivery
for every OS setting.
The linear scRGB and 80-nit reference follow Microsoft's
[Advanced Color guidance](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/high-dynamic-range).
The display-query ABI is checked against Windows SDK 10.0.26100.0; older APIs
that cannot distinguish WCG from HDR report an unknown Advanced Color mode.

Drag the slice or channel bars, or focus a bar and use arrow keys. Shift scales
keyboard steps by ten; Control scales them by one tenth. Enter commits a text
draft, and Escape cancels it or an active drag. Ctrl+C/V outside text entry copies
or applies a complete color. Inside text entry, the same shortcuts edit the draft.
Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y navigate color history outside text entry.
Committed colors, recent colors and picker settings are restored on startup.
Undo history and unfinished text/drag edits stay within the running session.
See [Saved state](STATE_FORMAT.md) for profile locations, the schema and failure behavior.

The workspace contains `okcolor-core` for device-independent color calculations
and `okcolor-app` for zgui and platform integration. zgui is pinned to an exact
revision in the workspace manifest; retain `Cargo.lock` when updating it.
The winit backend is patched locally to request the no-redirection style
at window creation; the renderer verifies the actual HWND before creating the
swap chain. See [the backend patch](../vendor/zgui-platform-winit/README.md) for
its scope and update procedure. Do not replace this with a post-creation style change.
The [shader vocabulary patch](../vendor/zgui-wgsl/README.md) preserves extended
RGB in custom paint while retaining upstream alpha clamping. The
[renderer patch](../vendor/zgui-render-wgpu/README.md) exposes its offscreen GPU
texture for the final conversion. All patches use the same pinned revision;
their READMEs describe the limited changes.
See [Color contracts](COLOR_CONTRACT.md) for input, output, gamut and history semantics.

The Windows caption bridge exposes the painted maximize button as `HTMAXBUTTON`
for Snap layouts. It uses the node's physical bounds from each completed frame,
invalidates them on resize/DPI changes, and forwards button gestures to zgui so
there is only one maximize/restore action. Native hover still reaches Windows.
The MSVC build embeds a Common Controls v6 manifest for the subclassing APIs.

Martian Mono is embedded under OFL. Martian Grotesk is an optional user-installed
font; set `OKCOLOR_GROTESK_FONT` to a licensed local TTF file to register it under
the application's family name. Keep machine-specific values outside the repository.
The supplied Grotesk EULA requires a separate license for creative-software
embedding, so it is not included in source or binaries. Label typography must
include a system sans-serif fallback.

## Product checks

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p okcolor-app --test composition --locked -- --ignored
cargo test -p okcolor-app --test color_surface --locked -- --ignored
cargo test -p okcolor-app --test picker --locked -- --ignored
cargo test -p okcolor-app --test native_caption --locked -- --ignored
cargo test -p okcolor-app --test wide_pipeline --locked -- --ignored
cargo test -p okcolor-app --test wide_picker --locked -- --ignored
cargo test -p okcolor-app --test native_display --locked -- --ignored
cargo test -p okcolor-app --lib windows_color_events --locked -- --ignored
```

The six explicit GPU tests fail if no device is available. The first three
check alpha composition, shader pixels against the f64 color reference, and
mounted input/clipboard/IME/drag behavior with idle-frame and DPI checks on the
headless platform. Set `OKCOLOR_TEST_RGBA` to an output path to capture the initial
330×560 UI as RGBA bytes. Readback occurs only in tests, never in the application's
per-frame path.
The Windows-only `native_caption` test opens its own temporary Picker window.
It checks native hit testing against painted bounds, capture, release-outside
cancellation, and a maximize/restore round trip through winit and zgui. It also
requires successfully presented frames at both maximized and restored extents. It sends
messages only to the test window and does not inject desktop input.
`wide_pipeline` reads FP16 GPU pixels after retained custom paint and the scRGB
conversion, comparing P3/Rec.2020 colors, alpha and white scaling against the
f64 core, including texture growth/shrink and both SDR output byte orders.
`wide_picker` checks the mounted slice, bars and samples through final scRGB
conversion while simulating SDR/WCG/HDR, unknown displays and an SDR fallback.
It also checks checkerboard alpha and finite output for extreme input. Simulated
display policies do not establish the behavior of an actual monitor transition.
`native_display` uses its own hidden window to read the current display
configuration without changing OS settings. It requires local display access;
remote sessions or mirrored displays may prevent an unambiguous query.
The `windows_color_events` test requires Windows 11 build 22621+ and a local
monitor. It registers and revokes real notifications on private hidden windows,
reads Advanced Color information, and repeats queue creation/shutdown. It checks
pending-work drainage, borrowed-queue ownership, callback-capture release and
preservation of the thread's quit message. It does not toggle HDR or generate
physical display-change events.

Native Windows validation must additionally cover IME, clipboard, DPI changes,
resize, minimize/restore, window movement and device loss. Headless pixel tests
do not establish those behaviors. Measure performance with a release build.
On Windows, the regular test suite also creates hidden native windows to verify
composition styles, caption message forwarding, negative screen coordinates,
and stale-layout rejection. Native tests do not establish the shell's Snap flyout,
hardware input, or interactive movement and resizing; verify those separately.

## Verification and publication

Check document links and `git diff --check` for documentation changes.
Run the product checks above after Rust changes.
