# Color model and editing contracts

`okcolor-core` contains no windowing, clipboard or networking code. Its public operations are usable without a graphics device.

## Source color

The editor stores OKLCH and alpha as `f64`. Lightness and alpha are in `0..=1`,
chroma is nonnegative, and hue wraps into `0..360`. There is no upper chroma
limit tied to the visible chart. A neutral RGB/Lab input retains the supplied
editing hue; an explicit OKLCH hue is retained even at zero chroma.

Conversion preserves extended RGB values. Membership, mapping and formatting
are separate operations. An extreme finite source can exceed the representable
range of a conversion; bounded clipping and formatting report an error instead
of emitting nonfinite text or corrupting the source.

The numerical reference is [CSS Color 4, 12 November 2026](https://www.w3.org/TR/2026/CRD-css-color-4-20261112/).
RGB matrices use D65; Lab/LCH use D50 with Bradford adaptation. In this pinned
version, `rec2020` uses display-referred gamma 2.4. It does not use the older
scene-referred transfer curve. Independent Color.js 0.7.1 fixtures cover D50
conversion and binary-search local-MINDE mapping.

## Absolute color input

| Function | Supported forms |
| --- | --- |
| HEX | 3, 4, 6 or 8 digits, with `#` |
| `rgb`, `rgba` | Modern spaces/slash or legacy commas; numbers and percentages |
| `hsl`, `hsla` | Modern spaces/slash or legacy commas; legacy S/L require percentages |
| `oklab`, `oklch`, `lab`, `lch` | Modern spaces/slash, numbers and percentages |
| `color` | `srgb`, `display-p3`, `rec2020`, with extended components |

Hue accepts degrees, `deg`, `grad`, `rad` and `turn`. CSS reference components
are clamped where specified, while `color()` RGB coordinates remain extended.
The resulting OKLCH lightness must fit the editor range. Number tokens are read
from their original spelling as `f64`; the tokenizer's `f32` values are not used.

This is an absolute-color subset, not a CSS declaration parser. Named colors,
`none`, expressions such as `calc()`, relative colors, variables, system colors
and `currentColor` are unsupported. A function needs its closing parenthesis;
trailing text, including trailing comments after a function, is rejected. Input
is limited to 4,096 bytes. Invalid text never mutates a committed color.

## Output and preview

Text output rounds to six decimal places; HEX rounds to 8-bit channels. HEX
automatically includes alpha when it is below one; HEX8 always includes it.
These formatted strings are for interchange, not lossless state persistence.

HEX, RGB and HSL are bounded sRGB outputs. Their result includes a `mapped` flag
when the source lies outside sRGB. `color()` output retains extended channels;
OKLCH, OKLab and Lab/LCH preserve their own coordinate representation.

`LocalMinde` implements the pinned specification's binary search with deltaEOK
JND 0.02 and epsilon 0.0001. The search caps its upper chroma bracket at one,
beyond all three supported SDR gamuts, to avoid overflow from extreme input.
`Clip` clamps destination RGB channels. Constant-lightness/hue chroma reduction
is a separate operation used for boundary search. No preview operation overwrites
the original color. Numeric gamut support does not establish monitor support.

Preview policy is runtime state. Active Windows WCG/HDR, a known reference white
and an FP16 presentation surface together enable extended RGB paint. SDR,
unknown display capabilities and 8-bit fallback surfaces use the selected sRGB
`LocalMinde` or `Clip` policy. Working-gamut boundaries remain independent of this
choice. Display changes do not create history entries or change saved colors.

All editing surfaces and current/previous/recent samples use this same policy.
Sample alpha is composited over a checkerboard. Bounded sRGB text output still
uses its mapping policy even while the preview uses extended RGB.

GPU preview precision is finite: chroma is capped at 10,000 for intermediate
arithmetic, and FP16 values saturate at ±65,504 instead of becoming infinities.
These representation limits affect extreme previews only; they are not a new
gamut boundary and never clamp the editor's `f64` source or serialized value.
An active wide output path does not promise complete P3/Rec.2020 panel coverage.

## Slice and history

A slice fixes hue, puts absolute chroma on the horizontal axis, and increases
lightness upward. Its chroma extent is explicit and does not change per sample.
Out-of-gamut clicks do not start a selection. Once a drag starts inside the gamut,
coordinates stay captured outside the chart: lightness follows the pointer within
0–1 and chroma is reduced to the selected gamut boundary at that lightness.
Reentering the gamut resumes direct selection. Markers are not clamped, so numeric/external colors can
remain outside the chart. Each gamut boundary is calculated independently;
Display P3 and Rec.2020 must not be forced into a nested order.

`History::begin/preview/commit` creates one undo entry for an entire drag or
external burst. Cancel restores the exact previous color. Only changed commits
add deduplicated recent colors, except numeric wheel edits, which remain undoable
and persistent without changing recents. Undo/redo do not add recents. A new commit clears
redo; cancel and unchanged commits preserve it. History and recent counts have
explicit bounds, and history navigation requires resolving the current edit.
