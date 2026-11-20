# Windows distribution candidates

Use Windows x64 with the pinned Rust/MSVC toolchain and cargo-about 0.9.3.
Commit the source changes first: binary and source identifiers must refer to the
same clean Git commit. See [license collection](../licenses/README.md) for setup
and the source/notice review procedure.

```powershell
python -m unittest discover -s scripts -p 'test_*.py' -v
python scripts/package_windows.py --output output/windows-candidate
```

If cargo-about is outside PATH, pass `--cargo-about <path-to-cargo-about.exe>`.
Choose a new output directory on every run. Outputs belong in ignored `output/`;
never commit binaries, archives, logs or private validation profiles.

The command builds `okcolor-app` with `--release --locked`, verifies x64 and the
Windows GUI subsystem, and records its imported DLLs. It collects the notice
bundle, verifies its commit and checksums, and adds `OkColorPicker.exe`,
`START_HERE.txt`, the Aseprite script, the update helper and `PACKAGE.json`. The output contains an unpacked package,
a ZIP with the same files and `SHA256SUMS.txt` for the ZIP. The ZIP has fixed
entry timestamps; reproducible binary builds are not guaranteed. Assembly
neither signs nor uploads the candidate.

The application requires the Microsoft Visual C++ v14 x64 runtime, including the
runtime used by the statically linked DXC compiler. The package links the
[official Microsoft runtime download](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
in its start instructions; it does not redistribute or install the runtime DLLs.
Use a runtime at least as recent as the MSVC tools used to build the application.

## Publish an updatable Windows package

Use a stable GitHub release tag matching the Cargo package version (for example
`v1.2.0`). Upload the ZIP from the packaging command with its original name.
The updater reads the latest published release of `sheetau/OkColorPicker`,
selects its Windows x64 ZIP and verifies its size and SHA-256 digest from
GitHub's release API. Drafts, prereleases and older or equal versions are skipped.
The manifest inside the ZIP must match the release version, target, commit
prefix and checksums of all package files.

Extract the package into a writable folder and run `OkColorPicker.exe`.
Startup checks run in the background. **Install now** closes the picker,
replaces the packaged files and reopens it. **Install on next launch** saves
the verified ZIP and retries installation on the next launch, including
offline. The helper stages files in `.okcolor-update` beside the executable,
replaces the executable last and rolls back file replacement errors.
Keep the package together and close other copies using the same installation.
The Cargo development executable uses its normal build and launch workflow.

## Validate the extracted candidate

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
