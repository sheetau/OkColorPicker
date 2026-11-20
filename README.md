# OkColorPicker

OkColorPicker is an OKLCH color picker for Windows. Explore a lightness, chroma
or hue slice, sample a color from the screen, and copy it as a CSS color. The
chart shows the sRGB, Display P3 and Rec.2020 boundaries. Wide-gamut colors keep
their original coordinates when you preview a mapped or clipped sRGB version.

## Getting started

Extract the Windows x64 ZIP to a writable folder and run **OkColorPicker.exe**.
Use a DirectX 12-capable GPU. If Windows reports a missing runtime DLL, install
the [Microsoft Visual C++ v14 x64 runtime](https://aka.ms/vc14/vc_redist.x64.exe).

- **Picker** edits two color slots with numeric fields, sliders and a two-dimensional chart.
- **Harmony** builds variations and palettes from the selected color.
- **APCA** generates text or background colors for a chosen contrast target.

All three tabs share your saved swatches and folders. The picker restores colors,
recent selections and preferences between sessions. See the [color tools guide](docs/TOOLS.md).

## Connect a painting app

Right-click the connection button in the status bar to choose an app. Left-click
its abbreviation to connect or disconnect. The picker remembers the app you chose.

| App | Setup | Colors |
| --- | --- | --- |
| CLIP STUDIO PAINT | Open the Companion Mode QR code on screen, then click **CSP**. [Pairing guide](docs/CSP_PROTOCOL.md) | Main and sub colors, both directions |
| Aseprite 1.3+ | Copy the included `OkColorPicker.lua` to **File → Scripts → Open Scripts Folder**. Rescan the scripts folder, run **OkColorPicker** from the Scripts menu and click **Ase** in the picker. | Foreground and background, both directions |
| Rebelle 8 Pro | Start Rebelle with `-websocket-server-enable -websocket-allowed-ip-addresses 127.0.0.1`, then click **Reb**. [Rebelle WebSocket settings](https://www.escapemotions.com/products/rebelle/manual/8.4/starting-painting/websocket/) | Selected picker color to the brush |

Aseprite and Rebelle communicate on this computer using ports 9009 and 8265
respectively. The first connection sends the current picker colors to the app.

## License

OkColorPicker is distributed under the [Apache License 2.0](LICENSE-APACHE).
Open **Settings → Licenses and sources** for the notices and source archives
included with the Windows package.
