# APCA contrast

The APCA tab compares a source hue with two editable opaque reference colors.
Each generated level has an editable contrast magnitude and a gamut limit.

Values use APCA 0.0.98G, ported from the MIT-licensed
[Color.js implementation](https://github.com/color-js/color.js/blob/091d5a6cb6f2a236fb65a67d16d6e2856494f2cf/src/contrast/APCA.js).
Copyright 2021 Lea Verou, Chris Lilley; the license is in
[`licenses/Colorjs-MIT.txt`](../licenses/Colorjs-MIT.txt).
The [Harmonizer](https://github.com/evilmartians/harmonizer) project informed the
workflow of adjusting OKLCH lightness toward a contrast target.

Positive Lc corresponds to dark text on a light background. Negative Lc
corresponds to light text on a dark background. The grid displays rounded
absolute Lc. The signed calculation remains available in the color core.

The calculator uses the selected RGB gamut and composites foreground alpha
in encoded RGB before applying the luminance exponent. Display P3 uses the
coefficients published in [APCA-W3](https://github.com/Myndex/apca-w3/blob/master/src/apca-w3.js).
The display's HDR mode does not enter the calculation. Candidate generation
searches both polarities at fixed hue, reducing chroma to fit the selected
gamut at each lightness. It selects the reachable result closest to the source
lightness and shows a dash for an unreachable target.

These are experimental APCA Lc values, not WCAG compliance results or a
typography recommendation. Font size, weight and viewing context still require
assessment. See [Why APCA](https://git.apcacontrast.com/documentation/WhyAPCA.html)
for the method and its intended use.
