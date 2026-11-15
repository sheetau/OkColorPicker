# Color tools

The title returns to the Picker. The **S**, **H** and **A** buttons open
Swatches, Harmonies and APCA. **Ctrl+1**, **Ctrl+2**, **Ctrl+3** and **Ctrl+4**
select those pages directly. **Ctrl+Tab** moves forward and **Ctrl+Shift+Tab**
moves backward. The main/sub palette remains available on every page.

## Swatches

Recent colors appear below the palette, followed by unfiled swatch tiles and
named folders. Click **+** to save the current original color. Click a tile to
recall it into the selected main/sub slot; this is one normal color Undo step.

**Edit** opens library controls. Select a tile, enter a name and click **Rename**.
Enter a name and click **New folder** to create a folder. Select a tile, choose
a destination in the editor and click **Move** to file it. A folder's heading
selects that folder for renaming/removal and makes it the destination for new
colors. **Unfiled** selects the top-level destination.

**Remove** deletes the selected swatch or folder. Removing a folder retains its
colors in the unfiled section. Library edits save automatically and are separate
from color Undo/redo. Names may contain 1–64 characters. The library supports
256 swatches and 32 folders.

## Harmonies

Shades interpolate toward black, tints toward white, and tones toward a neutral
at the same lightness, in OKLCH. Each strip contains seven steps. The harmony
button cycles through complementary, analogous, split complementary, triadic
and tetradic hue offsets. Generated colors preserve alpha and reduce chroma
as needed to fit the selected working gamut.

Click any generated color to use it. The strips then regenerate from that color.
**Save current to Swatches** stores it in the unfiled section. Undo restores the
previous selected color.

## APCA

The selected color is the text color. Set an opaque background with a CSS color,
choose black or white, or use the other main/sub slot. Positive Lc means dark
text on light; negative means light text on dark. Enter a signed target and
choose **Adjust L** to find a nearby lightness while keeping chroma, hue and
alpha. An unreachable target leaves the original unchanged.

See [APCA contrast](CONTRAST.md) for calculation assumptions and limitations.
The tab reports experimental Lc values and does not assign accessibility grades.
