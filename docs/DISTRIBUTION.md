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
`START_HERE.txt` and `PACKAGE.json`. The output contains an unpacked package,
a ZIP with the same files and `SHA256SUMS.txt` for the ZIP. The ZIP has fixed
entry timestamps; reproducible binary builds are not guaranteed. Assembly
neither signs nor uploads the candidate.

The application requires the Microsoft Visual C++ v14 x64 runtime, including the
runtime used by the statically linked DXC compiler. The package links the
[official Microsoft runtime download](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
in its start instructions; it does not redistribute or install the runtime DLLs.
Use a runtime at least as recent as the MSVC tools used to build the application.

## Validate the extracted candidate

Verify the ZIP against `SHA256SUMS.txt`, extract the entire archive into a fresh
directory, and compare the extracted files against `PACKAGE.json`. Run the
extracted executable from a path containing spaces and non-ASCII characters.
Set `OKCOLOR_STATE_DIR` to a separate temporary profile to preserve existing
saved colors. Explicit stdout/stderr redirection plus `OKCOLOR_TRACE_FRAMES=1`
allows startup and presentation checks without a console window.

Test without `OKCOLOR_GROTESK_FONT` to check the bundled/system fallback. A local
licensed Grotesk font can be used during development; choose its distribution
arrangement before release. It is not included in the package.

A successful launch on a development computer does not prove clean-machine
installation or hardware compatibility. Before publication, separately verify
runtime installation on a clean Windows machine, interactive input and IME,
clipboard, DPI/window transitions, CSP interoperability, display color modes,
and release performance. See [Development](DEVELOPMENT.md) for test scope and
remaining native checks. Preserve the notice and source files when distributing
the executable, and review the final artifact before publishing.
