# Windows distribution candidates

Use Windows x64 with the pinned Rust/MSVC toolchain, cargo-about 0.9.3 and Inno Setup 6.4.1.
Commit the source changes first: binary and source identifiers must refer to the
same clean Git commit. See [license collection](../licenses/README.md) for setup
and the source/notice review procedure.

```powershell
python -m unittest discover -s scripts -p 'test_*.py' -v
python scripts/package_windows.py --output output/windows-candidate
```

If cargo-about is outside PATH, pass `--cargo-about <path-to-cargo-about.exe>`.
Use `--iscc <path>` if ISCC.exe is outside PATH. Choose a new output directory
on every run. Outputs belong in ignored `output/`. Do not
commit binaries, archives, logs or private validation profiles.

The command builds `okcolor-app` with `--release --locked`, verifies x64 and the
Windows GUI subsystem, and records its imported DLLs. It collects the notice
bundle, verifies its commit and checksums, and adds `OkColorPicker.exe`,
`START_HERE.txt`, the Aseprite script, the Krita and Obsidian plugins, the update helper
and `PACKAGE.json`. The output contains the per-user Setup executable, an unpacked payload,
the ZIP used by portable installations and SHA256SUMS.txt covering both downloads.
Setup installs the same payload and registers a Start menu shortcut and an uninstaller.
When the x64 VC runtime is missing or old, Setup downloads the pinned Microsoft
redistributable over HTTPS, verifies its SHA-256 and requests elevation for
quiet installation. Runtime errors appear in Setup. Uninstall preserves the
shared runtime and the color profile.

## Publish an updatable Windows package

Use a stable GitHub release tag matching the Cargo package version. Upload the
Setup executable and the portable ZIP with their original names. Direct users to
the Setup executable. The updater reads the latest published release of `sheetau/OkColorPicker`,
selects Setup for a registered installation or the ZIP for a portable one,
and verifies the size and SHA-256 digest from
GitHub's release API. Drafts, prereleases and older or equal versions are skipped.
The manifest inside the ZIP must match the release version, target, commit
prefix and checksums of all package files.

Run Setup and start OkColorPicker from the Start menu.
Startup checks run in the background. **Install now** closes the picker,
replaces the packaged files and reopens it. **Install on next launch** saves
the verified download and retries installation on the next launch, including
offline. Registered installations use Setup and its prerequisite check.
For portable installs the helper stages files in `.okcolor-update` beside the executable,
replaces the executable last and rolls back file replacement errors.
Keep the package together and close other copies using the same installation.
The Cargo development executable uses its normal build and launch workflow.

## Validate Setup and the portable payload

Verify the Setup executable against SHA256SUMS.txt. Install it to a fresh directory
and compare the installed payload with PACKAGE.json. Confirm that the Start menu
shortcut resolves to the installed executable. Uninstall through Windows Settings
and check that the user profile is preserved. Exercise prerequisite download,
cancellation and restart on a disposable Windows system without the required runtime.


Verify the ZIP against `SHA256SUMS.txt`, extract the entire archive into a fresh
directory, and compare the extracted files against `PACKAGE.json`. Run the
extracted executable from a path containing spaces and non-ASCII characters.
Set `OKCOLOR_STATE_DIR` to a separate temporary profile to preserve existing
saved colors. Explicit stdout/stderr redirection plus `OKCOLOR_TRACE_FRAMES=1`
allows startup and presentation checks without a console window.

The executable embeds Martian Grotesk and Martian Mono. Both OFL notices are
included in the generated third-party notices.

A successful launch on a development computer does not prove clean-machine
installation or hardware compatibility. Before publication, separately verify
runtime installation on a clean Windows machine, interactive input and IME,
clipboard, DPI/window transitions, CSP interoperability, display color modes,
and release performance. See [Development](DEVELOPMENT.md) for test scope and
remaining native checks. Preserve the notice and source files when distributing
the executable, and review the final artifact before publishing.
