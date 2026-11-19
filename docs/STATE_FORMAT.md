# Saved state

On Windows the application uses `%LOCALAPPDATA%\OkColorPicker\state.json`.
`OKCOLOR_STATE_DIR` can select another absolute directory for a portable profile
or an isolated test. A relative directory is rejected. Development runs on other
platforms use `$XDG_STATE_HOME/OkColorPicker`, falling back to
`$HOME/.local/state/OkColorPicker`.

`state.lock` holds an operating-system file lock for the lifetime of
the writable session. Its presence alone does not mean a process is running;
the operating system releases the lock when the process exits. A second writer
opens a session without persistence and displays a warning.

## Version 4

The optional boolean `tools.always_on_top` defaults to false. When enabled,
the next session opens at the same window level.

The optional `tools.show_alpha_slider` defaults to false. The optional
`tools.contrast_grid` stores 1–16 target magnitudes (integer Lc magnitudes, stored range 0–110),
an optional row-aligned `positive_polarities` array (true, false or null for automatic),
the split index, an opaque second reference, contrast direction, gamut and
chroma policy. CSS suffixes follow row order in steps of 100 for up to nine levels
and 50 for larger grids. Legacy `level_names` can still be read. Profiles without a grid
use the initial four levels.

The UTF-8 JSON object contains:

| Field | Contract |
| --- | --- |
| `version` | Integer `4` |
| `main`, `sub` | Original `[L, C, H, alpha]` values, stored as f64 numbers |
| `selected` | `0` for main or `1` for sub |
| `recent` | Up to 24 color arrays, newest first |
| `show_p3`, `show_rec2020` | Independent visibility toggles; sRGB is always visible |
| `gamut` | `srgb`, `display-p3`, or `rec2020` |
| `mapping` | `local-minde` or `clip` |
| `format` | `hex`, `hex8`, `rgb`, `hsl`, `oklab`, `oklch`, `lab`, `lch`, or a gamut name for `color(...)` |
| `max_chroma` | Finite display extent from `0.01` through `1.0`; this does not cap the original color's chroma |

Color components follow the [color contracts](COLOR_CONTRACT.md). Serialization
and parsing preserve their f64 values, including alpha, wide-gamut values and the
editing hue of a neutral color. Duplicate recent colors are removed on restore,
retaining their first occurrence. Restoring recents creates no undo steps.
The `library` object contains `next_id`, `folders` and `swatches`. Each folder
has a unique integer `id` and `name`. Each swatch has `id`, `name`, original
`color` components and a `folder` ID or null. Names contain 1–64 characters;
there are at most 256 swatches and 32 folders. IDs are unique and below the
monotonic `next_id`. Removing a folder moves its colors to the unfiled section.

The `tools` object stores an opaque `background` color, signed `target_lc`
from -110 through 110, and a `harmony` index from 0 through 4. Tool selection,
field drafts and the active tab are transient; startup opens the picker.
The chart stores its fixed axis, snapping preference and height in CSS pixels
as `fixed_axis`, `always_snap` and `plot_height`. Older profiles default to Hue,
Shift-only snapping and 152 pixels. Library edits are independent of the session
color Undo history.

No clipboard contents, font paths, pairing credentials or undo stack are stored.

Version 1 migrates explicitly: its `current` becomes `main`, its `previous`
becomes `sub`, and `selected` starts at zero. Both original colors are preserved.
Version 2 migrates its selected gamut to visibility flags and resets the display extent to 0.37, or 0.47 for Rec.2020.
Version 3 gains an empty library and default tool settings.
A successful later save writes version 4; invalid legacy fields are still rejected.
Older application versions cannot read a version 4 profile.

The schema rejects invalid components, unknown fields/choices, unsupported
versions and files larger than 256 KiB. A missing file starts a fresh profile.
An unreadable or invalid file remains untouched: the picker opens with defaults,
displays a session-only warning, and disables automatic saving for that run.
Close the application before restoring a known-good copy or moving an invalid
file aside. Future schema versions must define migration explicitly.

## Save lifecycle

Only committed colors, the selected slot, library and preferences are submitted to the writer. During a
drag, preference changes still save the last committed color. Undo/redo changes
the saved color pair while leaving recent-color ordering intact.
Numeric wheel adjustments likewise save the current color without changing
recents. CSP gesture samples remain previews until release; a disconnect, local
takeover or normal shutdown commits the last received sample as one edit.

One background worker keeps the latest pending snapshot. It sleeps when idle,
does not perform disk I/O on the drawing thread, and skips unchanged snapshots.
Each save validates and serializes first, writes a temporary file in the profile
directory, synchronizes its contents, then atomically replaces `state.json`.
On Windows this uses the pinned tempfile implementation's `MoveFileExW` replace
operation. Unix builds also synchronize the directory after replacement. The
filesystem and storage device determine power-loss durability.

A failed write/replacement preserves the previous file, reports the error in
the picker and offers **Retry save**. Further edits also retry with the latest
snapshot. Normal window shutdown captures the final committed state and joins
the writer before releasing the profile lock. Forced termination can lose a
pending edit; incomplete temporary files are ignored on the next startup.

## Validation

```powershell
cargo test -p okcolor-app --test persistence --test model --locked
cargo test -p okcolor-app --test picker --locked -- --ignored
```

The filesystem tests use temporary profiles, including Windows replacement
failure and retry checks. The GPU-backed mounted picker test covers autosave,
shutdown immediately after a commit, profile-lock release and state restoration.
