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

The surface currently uses an 8-bit SDR format. Supporting Display P3 and
Rec.2020 calculations does not establish wide-gamut monitor output. Treat the
window as an sRGB preview until the display path is explicitly validated.

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
The winit backend alone is patched locally to request the no-redirection style
at window creation; the renderer verifies the actual HWND before creating the
swap chain. See [the backend patch](../vendor/zgui-platform-winit/README.md) for
its scope and update procedure. Do not replace this with a post-creation style change.
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
```

The four explicit GPU tests fail if no device is available. The first three
check alpha composition, shader pixels against the f64 color reference, and
mounted input/clipboard/IME/drag behavior with idle-frame and DPI checks on the
headless platform. Set `OKCOLOR_TEST_RGBA` to an output path to capture the initial
330×560 UI as RGBA bytes. Readback occurs only in tests, never in the application's
per-frame path.
The Windows-only `native_caption` test opens its own temporary Picker window.
It checks native hit testing against painted bounds, capture, release-outside
cancellation, and a maximize/restore round trip through winit and zgui. It sends
messages only to the test window and does not inject desktop input.

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
