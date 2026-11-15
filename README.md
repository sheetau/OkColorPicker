# OkColorPicker

An OKLCH color picker for Windows, built in Rust with zgui.

Explore a fixed-hue lightness/chroma slice, adjust L/C/H/alpha, and edit or copy
CSS color values. The picker preserves the original color while displaying
sRGB, Display P3 and Rec.2020 boundaries. The preview uses the detected Windows
display mode and falls back to sRGB when extended color output is unavailable.

Swatches stores named colors and folders. Harmonies generates shades, tints,
tones and hue harmonies in the working gamut. The APCA tab compares text colors
and adjusts lightness toward a signed contrast target.
See [color tools](docs/TOOLS.md) for usage.

The application saves committed colors, recent colors, swatches and tool preferences
between sessions. Undo history stays within the current session.
Connect to CLIP STUDIO PAINT through Companion Mode to receive and send color
edits while preserving the picker's alpha. See [CSP pairing and synchronization](docs/CSP_PROTOCOL.md)
for the connection steps and behavior.

The application is in development; there is no release yet. Physical HDR/WCG
monitor behavior and clean-machine installation remain unverified.

## Development

From the repository root, build and launch the current source:

```powershell
cargo run -p okcolor-app --release
```

See [Development](docs/DEVELOPMENT.md) for the local toolchain and checks.

## License

Licensed under the [Apache License 2.0](LICENSE-APACHE).
