# APCA contrast

The APCA tab evaluates the selected main or sub color as text against an opaque
background. White and black comparisons remain visible. The other slot can be
used as the background without changing either original color.

Values use APCA 0.0.98G, ported from the MIT-licensed
[Color.js implementation](https://github.com/color-js/color.js/blob/091d5a6cb6f2a236fb65a67d16d6e2856494f2cf/src/contrast/APCA.js).
Copyright 2021 Lea Verou, Chris Lilley; the license is in
[`licenses/Colorjs-MIT.txt`](../licenses/Colorjs-MIT.txt).
The [Harmonizer](https://github.com/evilmartians/harmonizer) project informed the
workflow of adjusting OKLCH lightness toward a contrast target.

Positive Lc means dark text on a light background; negative Lc means light text
on a dark background. The calculation uses bounded sRGB after the selected
clipping or local-MINDE mapping. Transparent text is composited over the opaque
background in encoded sRGB. Display HDR state does not change the calculation.
An unrepresentable color produces an unavailable result, never a fabricated score.

**Adjust L** searches for the closest lightness to the original that reaches the
signed target within 0.01 Lc, preserving chroma, hue and alpha exactly. The
search checks intervals throughout the lightness range because gamut mapping
can make the response non-monotonic. Some targets are unreachable, including
the low-contrast discontinuity and targets beyond the available contrast with
transparency. Failure leaves the original color untouched. A successful change
is one normal color edit and can be undone.

These are experimental APCA Lc values, not WCAG compliance results or a
typography recommendation. Font size, weight and viewing context still require
assessment. See [Why APCA](https://git.apcacontrast.com/documentation/WhyAPCA.html)
for the method and its intended use.
