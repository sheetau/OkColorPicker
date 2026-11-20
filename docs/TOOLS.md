# Color tools

Use the three buttons in the status bar or **Ctrl+1/2/3** to select Picker,
Harmony or APCA. **Ctrl+Tab** and **Ctrl+Shift+Tab** cycle through them. Every
page includes recent colors, main/sub slots, L/C/H/alpha fields, editable color
codes and the swatch library. The window fits the active page. Resize the Picker
vertically to change the chart height. Returning to Picker restores that height.
Drag the title to move the window. Settings covers the current page and scrolls
when its controls exceed the available height. Dropdowns end 12 px left of the center of the opening
chevron or palette settings icon. Options align vertically with the trigger as far
as the page permits.
Palette adjustment options sit immediately above the palette swatches.

## Picker

The chart holds one component constant. In Settings, **Lightness** shows the cone (C × L),
**Hue** the band (H × L), and **Chroma** the mountain profile (H × C).
The slider edits the component held constant in that view. Hold Shift to snap to a visible gamut boundary
on the chart or slider. **Always snap to gamut boundaries** reverses the Shift
behavior. Settings controls the gamut boundaries, the alpha slider and
**Always on top**. Press **L**, **C**, **H** or **A** outside a text field to select the corresponding numeric value for replacement. Press **O** or **R** outside a text
field to select the corresponding code for replacement. Escape discards an
uncommitted field edit and releases text focus. The eyedropper
(**Ctrl+Shift+C**) samples a visible SDR desktop pixel across Windows displays.
A floating chip previews the sampled pixel and its six-digit HEX value.
Hold either **Alt** key before moving over a hover-sensitive element to freeze
the visible desktop. The chip then reads the fixed image at the pointer.
Releasing both Alt keys drops the snapshot from memory and resumes live sampling.
Selecting a color or cancelling also discards it. The snapshot is displayed
at each monitor's physical resolution. On Advanced Color displays the overlay
uses a 16-bit floating-point scRGB capture and swap chain. The chip continues
to report the Windows SDR pixel value. Windows capture restrictions apply to
protected surfaces.
A left click selects the color and consumes the click. Right-click or Escape cancels sampling.

An unavailable slot shows a gray frame and cross on black. The active slot shows
the mapped sRGB sample and adds the clipped sample when their displayed colors
differ by at least 5 CIEDE2000 units. Click a sample to adopt that color
as one undoable edit. Until then the code fields and Harmony use the mapped
preview. Right-click the original slot to copy the stored color in the selected
output format.

## Swatches

Use the library heading to select unfiled tiles or a named folder.
Click the trailing **+** to save the current color. Drag a sample onto a tile
to insert before it, or onto **+** to append it. Drag within the library to
reorder. Middle-click a recent or library tile to remove it. Right-click a
color to copy it in the R field's selected format.

Choose **+ New folder** and type its name in the heading. Enter or leaving
the field saves the name. Escape or an empty name cancels the edit. Hold a
folder heading or a saved swatch still for 650 ms to rename it. Middle-click
a folder heading to remove it. For a populated folder, **Yes** removes the
folder and keeps its colors unfiled. **No** or Escape cancels.

While dragging a color, hover over the heading for 650 ms to open the folder
menu. Release on a folder or **Swatch Tiles** to move a saved swatch there or
to add a copy of another color. Ctrl+Z and Ctrl+Y undo and redo library,
palette, APCA and color edits in the order they occurred.

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
as anchors. Each generation compares 64 OKLCH candidates across eight compositions,
including tonal, pastel, accent and multihue arrangements. Starting colors are
sampled from occupied bands in the bundled named-color dictionary. Selection
accounts for near duplicates, neighboring colors, lightness range, accent strength
and the last eight generated palettes. Locked colors retain their positions and
original components, including alpha. New colors fit the selected working gamut.

Use the adjustment button to the left of Generate to edit the current palette.
Drag L, C, H or T horizontally. L, C and T display signed offsets from
−100 to +100. H displays degrees from −360 to +360.
L adds up to one unit of OKLCH lightness. C moves toward neutral at −100 or the
working gamut boundary at +100, at the adjusted lightness and hue. H rotates up
to one full turn in either direction. T mixes toward cool blue or warm red.
Locked colors and alpha are preserved. Generation and palette tile actions are
suspended while the menu is open.
Choose **Apply** to keep all adjustments as one undo step, or **Cancel** or
**Escape** to restore the original palette. Clicking outside closes an
unmodified menu. After an adjustment, the menu stays open until you decide.

Shift-click a palette tile to toggle its lock. A dot at the lower
center marks a locked tile. Click **+** to insert one color. Drag **+** upward
to choose the number of intermediate colors, one per 12 CSS pixels. Drag back
down to reduce the count. Returning to **+** cancels the insertion on release.
sRGB gradients interpolate the encoded RGB channels.
Drag to reorder, middle-click to remove and use the trailing **+** to append.
Hover a palette color to see its HEX value and closest color-names dictionary entry.
The generated palette stays in the current session.

## APCA

Edit an opaque reference color and the target Lc for each level. The reset button
beside Copy restores the default levels, split, reference colors and grid options
in one undoable edit.
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
