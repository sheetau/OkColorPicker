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

## Version 1

The UTF-8 JSON object contains:

| Field | Contract |
| --- | --- |
| `version` | Integer `1` |
| `current`, `previous` | Original `[L, C, H, alpha]` values, stored as f64 numbers |
| `recent` | Up to 24 color arrays, newest first |
| `gamut` | `srgb`, `display-p3`, or `rec2020` |
| `mapping` | `local-minde` or `clip` |
| `format` | `hex`, `hex8`, `rgb`, `hsl`, `oklab`, `oklch`, `lab`, `lch`, or a gamut name for `color(...)` |
| `max_chroma` | Finite display extent from `0.01` through `1.0`; this does not cap the original color's chroma |

Color components follow the [color contracts](COLOR_CONTRACT.md). Serialization
and parsing preserve their f64 values, including alpha, wide-gamut values and the
editing hue of a neutral color. Duplicate recent colors are removed on restore,
retaining their first occurrence. Restoring recents creates no undo steps.
No clipboard contents, font paths, pairing credentials or undo stack are stored.

The schema rejects invalid components, unknown fields/choices, unsupported
versions and files larger than 128 KiB. A missing file starts a fresh profile.
An unreadable or invalid file remains untouched: the picker opens with defaults,
displays a session-only warning, and disables automatic saving for that run.
Close the application before restoring a known-good copy or moving an invalid
file aside. Future schema versions must define migration explicitly.

## Save lifecycle

Only committed colors and preferences are submitted to the writer. During a
drag, preference changes still save the last committed color. Undo/redo changes
the saved current color while leaving recent-color ordering intact.
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
