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
cargo run -p okcolor-app --release
```

Run this from the repository root. Cargo rebuilds changed source and launches
the development executable directly. Omit `--release` for a debug build; add
`--locked` when checking that dependency resolution leaves Cargo.lock unchanged.

To keep a local Grotesk font configured across terminal sessions, create the
ignored `.cargo/config.toml` with your licensed font's absolute path:

```toml
[env]
OKCOLOR_GROTESK_FONT = { value = 'C:\Fonts\MartianGrotesk.ttf', force = false }
```

Cargo passes this setting to the launched application. An existing environment
variable takes precedence. The font is read at runtime, not embedded in the
executable. Without this setting the system sans-serif fallback is used.
The normal application profile persists between runs; `OKCOLOR_STATE_DIR` can
select a separate profile as described in [Saved state](STATE_FORMAT.md).

The executable uses DX12, DirectComposition, `WS_EX_NOREDIRECTIONBITMAP`,
premultiplied surface alpha and an opaque CSS background. A statically linked DXC
avoids loading unrelated `dxcompiler.dll` files from PATH. It logs the
selected adapter and actual surface formats; set `OKCOLOR_TRACE_FRAMES` to log
frame counts. It requires a working DX12 device. A device-loss callback marks the
device unavailable and wakes the window; recovery runs on the UI thread.

Windows release builds use the GUI subsystem, so launching the executable from
Explorer does not create a console window. A fatal application error is shown in
a native dialog before exit code 1. Debug builds retain the console. To capture
release diagnostics, start the process with explicit standard-output/error
redirection (for example, PowerShell `Start-Process -PassThru` with
`-RedirectStandardOutput` and `-RedirectStandardError`). Logs are not written to
the profile automatically.

The Windows presentation path composes into retained FP16 textures, then converts
premultiplied gamma-encoded values to linear scRGB on the GPU. It prefers an FP16
DirectComposition swap chain when the surface supports it and the display's
reference white is known. Otherwise it uses an unencoded 8-bit SDR surface and
copies the original gamma-encoded values. Alpha stays premultiplied in both paths.
Resize replaces the offscreen output and its binding without replacing the UI
renderer or its atlas. Surface acquisition failures preserve the submitted frame
for retry. Device loss recreates the DX12 device, FP16 composition, vector
rasterizer, output pass and native swap chain with the same compiler/presentation
policy. Failed attempts wait 250 ms, 1 s, 2 s, then at most one attempt every 5 s;
retry timers hold only a weak window reference and stop affecting closed windows.
The runtime invalidates dependent paint/atlas caches after recovery, retaining the
mounted view, focus and model. Uploads to a known lost device are refused until
recovery, leaving pending CPU data available.

The slice, four channel bars, main/sub samples and recent samples all use
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

Drag the slice or channel bars, or focus a bar and use arrow keys. Numeric fields
also accept Up/Down while focused, plus vertical wheel input when the pointer is
over a field, without requiring focus. Drag a numeric field horizontally to scrub
its value (four CSS pixels per step, with a three-pixel activation threshold);
a plain click enters text editing. One scrub is one undo action, and Escape
cancels it. Steps are
0.01 for L/alpha, 0.001 for C and 1 degree for H. Shift scales steps by ten;
Control scales them by one tenth. Wheel input accumulates one step per reported
line or 22 CSS pixels, independent of DPI; gesture momentum is ignored. Valid
wheel adjustments remain undoable and saved, but do not add recent colors. Valid
drafts are adjusted and committed as one history action; invalid drafts and IME
composition do not change the color. Untouched fields step from full-precision
model values, with four displayed decimal places for every channel.
L/alpha stop at their limits, C stops at zero, and hue wraps. Enter commits a text
draft, and Escape cancels it or an active drag. Ctrl+C/V outside text entry copies
or applies a complete color. Inside text entry, the same shortcuts edit the draft.
Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y navigate color history outside text entry.
Committed colors, recent colors and picker settings are restored on startup.
Undo history and unfinished text/drag edits stay within the running session.
See [Saved state](STATE_FORMAT.md) for profile locations, the schema and failure behavior.

The workspace contains `okcolor-core` for device-independent color calculations
and `okcolor-app` for zgui and platform integration. `okcolor-csp` contains the
UI-independent companion protocol and bounded blocking socket session; see
[CSP protocol](CSP_PROTOCOL.md) for its contracts and integration responsibilities. zgui is pinned to an exact
revision in the workspace manifest; retain `Cargo.lock` when updating it.
The winit backend is patched locally to request the no-redirection style
at window creation; the renderer verifies the actual HWND before creating the
swap chain. See [the backend patch](../vendor/zgui-platform-winit/README.md) for
its scope and update procedure. Do not replace this with a post-creation style change.
The [shader vocabulary patch](../vendor/zgui-wgsl/README.md) preserves extended
RGB in custom paint while retaining upstream alpha clamping. The
[renderer patch](../vendor/zgui-render-wgpu/README.md) exposes its offscreen GPU
texture for the final conversion. All patches use the same pinned revision;
their READMEs describe the limited changes. The
[runtime patch](../vendor/zgui-runtime/README.md) invalidates dependent caches on
`FrameOutcome::Recovered`; replaying old chunks alone cannot restore lost textures.
See [Color contracts](COLOR_CONTRACT.md) for input, output, gamut and history semantics.

On Windows, DWM owns the window border, shadow and rounded corners. The normal
resizable native frame is retained while `WM_NCCALCSIZE` replaces its title bar
with the application caption. Maximized client content begins at the visible
top edge; the first pixel participates in caption dragging. Side/bottom resizing
uses native hit testing, with a top resize band on restored windows. The product
does not paint an extra window border or use Windows-side CSS resize grips.
The settings menu independently toggles P3 and Rec.2020. sRGB stays visible;
the largest enabled gamut controls selection. The chroma extent is 0.37, or
0.47 while Rec.2020 is enabled, on both the slice and channel bar.
Vertical resizing changes the gamut height; the remaining controls keep their
content height, with no outer scrollbars.

The Windows caption bridge exposes the painted controls as `HTMINBUTTON`,
`HTMAXBUTTON` and `HTCLOSE`. It uses physical bounds from each completed frame
and invalidates them on resize/DPI changes. Win32 owns capture, outside-release
cancellation and queued system commands; zgui paints the shared native state
without a second pointer target or extra keyboard focus stops. Native hover still
reaches Windows for Snap layouts. The 36-by-31 CSS-pixel controls sit in a 32-pixel
title bar and use the installed Segoe Fluent Icons on Windows 11, or Segoe MDL2
Assets on older Windows. The four minimize/maximize/restore/close glyphs retain
towavue's installed icon family, with an explicit ten-pixel line box centered in
each control. Like the other icon buttons, their ink is #424242 normally and
#e0e0e0 on hover, without a hover background. No replacement symbols are drawn
and no Windows font files are distributed.
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
cargo test -p okcolor-app --test native_input --locked -- --ignored
cargo test -p okcolor-app --test native_recovery --locked -- --ignored
cargo test -p okcolor-app --test wide_pipeline --locked -- --ignored
cargo test -p okcolor-app --test wide_picker --locked -- --ignored
cargo test -p okcolor-app --test native_display --locked -- --ignored
cargo test -p okcolor-app --lib windows_color_events --locked -- --ignored
```

The seven explicit GPU tests fail if no device is available. The first three
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
`native_input` sends pointer, wheel and queued arrow-key messages through the
owned HWND, winit and the mounted Picker. It verifies captured dragging beyond
the slice bounds, release history, focused numeric steps, unchanged recent colors
after wheel input, window movement and a presented frame at the resized extent.
It does not inject global input, modify the clipboard or load the user profile;
hardware input and native IME behavior remain separate checks.
`native_recovery` destroys its own logical DX12 device twice from a background
thread after the UI has time to park. The production callback must wake it. After
each replacement, the test requires a native presentation and matching mounted
Picker pixels, including glyphs/custom paint, and verifies that the old device is
released. An active color drag and committed Undo/Redo history must survive. This
does not reset the display driver; physical GPU removal, driver reset and a period
with no usable adapter still require separate native validation.
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
composition styles, native caption commands, negative screen coordinates,
and stale-layout rejection. Native tests do not establish the shell's Snap flyout,
hardware input, or interactive movement and resizing; verify those separately.

## Repeatable rendering measurements

```powershell
cargo test -p okcolor-app --test performance --release --locked -- --ignored --nocapture
```

Run this measurement alone on a DX12 adapter supporting timestamp queries.
It dispatches mounted L/C/H/alpha and L-C slice drags across all three working
gamuts at 100% and 200% scale. Each of the 30 workloads warms up for 20 updates,
then measures 120 updates. Every measured event must change the color and produce
a frame; the UI must stop rendering after release. The benchmark has no numeric
speed threshold because adapters and system load vary.

JSON lines report p50/p95/max wall time from event dispatch through GPU completion,
including FP16 composition and the final scRGB pass. Each frame waits for its
GPU submissions, so this is serialized workload cost, not pipelined throughput
or native input-to-display latency. The separate GPU timing is the sum of zgui's
timed passes: it excludes the final scRGB pass and copies, and vector spans can
include CPU submission gaps. Only timestamp data is read back, never pixels.
Renderer-accounted allocation bytes and the extra scRGB target are reported
separately; they do not represent total driver/GPU residency or process memory.

The harness uses the production picker and output shader on a headless surface.
It excludes OS input, DirectComposition presentation, CSP and persistence, and
does not establish physical display behavior. Record the adapter, toolchain,
commit and font configuration with private results; compare equivalent settings.
Keep measurement output under ignored `output/`. Native validation and idle
CPU/GPU/process-memory measurements remain separate checks.

## Verification and publication

For local Windows candidate packaging, follow [Distribution](DISTRIBUTION.md).
The packager builds the locked release executable and combines it with verified
notices, corresponding source, instructions and checksums.

Before assembling a distribution, collect and review the
[license materials](../licenses/README.md), including notices for the statically
linked compiler and corresponding source for covered dependencies.

Check document links and `git diff --check` for documentation changes.
Run the product checks above after Rust changes.
