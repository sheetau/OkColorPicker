# Color tools

Use the three buttons in the status bar or **Ctrl+1/2/3** to select Picker,
Harmony or APCA. **Ctrl+Tab** and **Ctrl+Shift+Tab** cycle through them. Every
page includes recent colors, main/sub slots, L/C/H/alpha fields, editable color
codes and the swatch library. The window fits the active page. Resize the Picker
vertically to change the chart height. Returning to Picker restores that height.
Drag the title to move the window. Settings covers the current page and scrolls
when its controls exceed the available height.

## Picker

The chart holds one component constant. Choose L, C or H in Settings to change
the fixed component and its slider. Hold Shift to snap to a visible gamut boundary
on the chart or slider. **Always snap to gamut boundaries** reverses the Shift
behavior. Settings controls the gamut boundaries, the alpha slider and
**Always on top**. Press **O** or **R** outside a text
field to select the corresponding code for replacement. Escape discards an
uncommitted field edit and releases text focus. The eyedropper
(**Ctrl+Shift+C**) samples a visible SDR desktop pixel across Windows displays.
A floating chip previews the sampled pixel and its six-digit HEX value.
A left click selects the color and consumes the click. Right-click or Escape cancels sampling.

An unavailable slot shows a fixed-color frame and cross over the mapped fill. The active
slot offers mapped and clipped sRGB samples beside it. Click a sample to adopt that color
as one undoable edit. Until then the code fields and Harmony use the mapped
preview. Right-click the original slot to copy the stored color in the selected
output format.

## Swatches

Use the library heading to select unfiled tiles or a named folder.
Click the trailing **+** to save the current color. Drag a sample onto a tile
to insert before it, or onto **+** to append it. Drag within the library to
reorder. Middle-click a recent or library tile to remove it. Right-click a
color to copy it in the R field's selected format.

The folder menu opens the library editor. Select a tile to edit its name,
or enter a name and choose **New folder**. Use **Move** to change the folder
of the selected tile. Removing a folder keeps its colors in the unfiled
section. Library changes save independently of color undo.

## Harmony

Click a group heading to collapse or expand it. Each variation has thirteen samples with the source in the middle.
The four rows vary lightness (shades through tints), chroma, hue and temperature.
Similar shows up to six named sRGB colors from the bundled meodai/color-names
dictionary, ordered by CIEDE2000 distance in D50 Lab. It excludes the input
HEX and distances of 30 or more. Harmony shows analogous, complementary, split
complementary, triadic, tetradic and square combinations. Bounded sRGB
samples use the RGB color wheel. The wide-gamut tools work in OKLCH.
Click a sample to select it or drag it to the library.

**Generate** or **Space** replaces unlocked palette colors using locked colors
as anchors. Shift-click a palette tile to toggle its lock. A dot at the lower
center marks a locked tile. Click **+** to insert one color. Drag **+** upward
to choose the number of intermediate colors, one per 12 CSS pixels. Drag back
down to reduce the count. Returning to **+** cancels the insertion on release.
sRGB gradients interpolate the encoded RGB channels.
Drag to reorder, middle-click to remove and use the trailing **+** to append.
Hover a palette color to see its HEX value and closest CSS color name.
The generated palette stays in the current session.

## APCA

Edit an opaque reference color and the target Lc for each level.
**To text** generates backgrounds for the reference text. **To background**
generates text for the reference background. Choose sRGB or Display P3.
**Current C** limits chroma to the selected color's chroma and **Max C**
uses the available gamut at the generated lightness and hue.

Use **+** between two levels to insert their midpoint. Middle-click a row to remove it. Target Lc values round to whole numbers and clamp to the closest reachable score for that reference, polarity and gamut. The fields show
magnitudes. Positive APCA denotes dark text on light and negative APCA denotes
light text on dark. The sRGB extremes round to +106 and −108. Editing the
current color's Lc adjusts its lightness against that reference. Ctrl+Z and
Ctrl+Y navigate grid changes and color edits in the order they occurred. Drag the horizontal boundary to change the reference
for adjacent levels. Drag it to an edge to remove that region. Its **+**
restores the split. Click a candidate to adopt it or use **Copy** to copy
the available levels as CSS custom properties.

See [APCA contrast](CONTRAST.md) for the calculation and its limits.
