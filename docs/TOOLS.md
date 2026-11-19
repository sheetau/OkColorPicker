# Color tools

Use the three buttons in the status bar or **Ctrl+1/2/3** to select Picker,
Harmony or APCA. **Ctrl+Tab** and **Ctrl+Shift+Tab** cycle through them. Every
page includes recent colors, main/sub slots, L/C/H/alpha fields, editable color
codes and the swatch library. Scroll the body to reach lower groups at smaller
window sizes. Drag the title to move the window. The menu opens Settings.

## Picker

The crosshairs mark lightness and chroma. The hue bar hatches colors outside
the working gamut. Settings controls the gamut boundaries, alpha slider,
sRGB conversion and **Always on top**. Press **O** or **R** outside a text
field to select the corresponding code for replacement. The eyedropper
(**Ctrl+Shift+C**) samples a visible pixel inside the Windows application.
Right-click or Escape cancels sampling.

In sRGB preview, an unrepresentable slot shows a cross. The active slot offers
mapped and clipped sRGB samples beside it. Click a sample to adopt that color
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
Similar shows up to four named sRGB colors from the bundled meodai/color-names
dictionary, ordered by CIEDE2000 distance in D50 Lab. It excludes the input
HEX and distances of 30 or more. Harmony shows analogous, complementary, split
complementary, triadic, tetradic and square combinations. Bounded sRGB
samples use the RGB color wheel. The wide-gamut tools work in OKLCH.
Click a sample to select it or drag it to the library.

**Generate** or **Space** replaces unlocked palette colors using locked colors
as anchors. Hold the insertion control between two colors to choose the number
of intermediate samples. sRGB gradients interpolate the encoded RGB channels.
Drag to reorder, middle-click to remove and use the trailing **+** to append.
Hover a palette color to see its HEX value and closest CSS color name.
The generated palette stays in the current session.

## APCA

Edit an opaque reference color and the target Lc for each level.
**To text** generates backgrounds for the reference text. **To background**
generates text for the reference background. Choose sRGB or Display P3.
**Current C** limits chroma to the selected color's chroma and **Max C**
uses the available gamut at the generated lightness and hue.

Use **+** between two levels to insert their midpoint. Middle-click a row to remove it. Target Lc values round to whole numbers and clamp to 0–110. Editing the
current color's Lc adjusts its lightness against that reference. Ctrl+Z and
Ctrl+Y navigate grid changes and color edits in the order they occurred. Drag the horizontal boundary to change the reference
for adjacent levels. Drag it to an edge to remove that region. Its **+**
restores the split. Click a candidate to adopt it or use **Copy** to copy
the available levels as CSS custom properties.

See [APCA contrast](CONTRAST.md) for the calculation and its limits.
