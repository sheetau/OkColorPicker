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

The current executable is a rendering probe. It uses DX12, DirectComposition,
premultiplied surface alpha and an opaque CSS background. A statically linked DXC
avoids loading unrelated `dxcompiler.dll` files from PATH. The probe logs the
selected adapter, actual surface formats and frame count. It requires a working
DX12 device and does not yet recover from device loss.

The surface currently uses an 8-bit SDR format. Supporting Display P3 and
Rec.2020 calculations does not establish wide-gamut monitor output. Treat the
probe as an sRGB preview until the display path is explicitly validated.

The workspace contains `okcolor-core` for device-independent color calculations
and `okcolor-app` for zgui and platform integration. zgui is pinned to an exact
revision in the workspace manifest; retain `Cargo.lock` when updating it.
See [Color contracts](COLOR_CONTRACT.md) for input, output, gamut and history semantics.

## Product checks

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p okcolor-app --test composition --locked -- --ignored
```

The last command requires a GPU. It fails if no device is available and checks
opaque, transparent and premultiplied pixels by readback, plus idle frame
behavior. It is separate from portable core tests. Readback occurs only in the
test, never in the application's per-frame path.

Native Windows validation must additionally cover IME, clipboard, DPI changes,
resize, minimize/restore, window movement and device loss. Headless pixel tests
do not establish those behaviors. Measure performance with a release build.

## Verification and publication

Check document links and `git diff --check` for documentation changes.
Run the product checks above after Rust changes.
