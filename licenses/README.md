# Distribution license materials

`NOTICE` at the repository root introduces the embedded components. This folder
keeps upstream notices for zgui, Martian Mono, the CSP protocol reference and
[static DXC](dxc/README.md). Local zgui patches retain their own LICENSE/NOTICE
files. Preserve upstream text, copyright lines and the marked modifications.

## Rust dependency collection

Use [cargo-about](https://embarkstudios.github.io/cargo-about/) version 0.9.3 with
the locked application graph. From the repository root:

```powershell
cargo install cargo-about --version 0.9.3 --locked --features cli
New-Item -ItemType Directory -Force output/licenses | Out-Null
cargo about generate --locked --fail --manifest-path crates/okcolor-app/Cargo.toml --config licenses/about.toml --format json --output-file output/licenses/dependencies.json
```

The configuration filters for Windows x64, omits development-only dependencies
and retains build dependencies. Its clarifications use SHA-256 of the original
files. Git clarifications resolve against each package's `.cargo_vcs_info.json`;
their comments record the revisions used when reviewing the locked versions.
For compound license files such as libm's, the complete original text is retained.

Use an online collection when clarifications require upstream files. Offline
collection can substitute generic license text. Even a successful `--fail` run
can contain generic text with placeholder copyright fields. Inspect entries
without `source_path`, their `used_by` packages and the actual text before
including the result in a release. Do not replace an unknown copyright holder
with the package's author by assumption. Review upstream source notices and
preserve the package's original license expression when adding clarifications.

The raw JSON includes local Cargo paths and is an ignored review artifact. A
distributed notice document must contain the license texts and package/version
attribution without those machine-specific paths. The selected license list is
not a release approval or a substitute for the upstream obligations.

## Assemble notices and source

After committing the source and reviewing the collection, run:

```powershell
python -m unittest discover -s scripts -p test_assemble_notices.py -v
python scripts/assemble_notices.py --output output/notices
```

Use `--cargo-about <path>` if the pinned tool is installed outside PATH. The
output directory must not exist. Assembly requires a clean Git worktree and
collects a fresh locked report. It writes a self-contained notice document,
`SOURCES.zip`, a package-to-source index, DXC source provenance and a checksum
manifest. It publishes nothing. The raw report and local Cargo paths are not
included in the output.

Registry source archives must match Cargo.lock checksums. Git dependencies are
archived at their locked commits; local application and patch sources come from
the current project commit. Git excludes untracked files. These are source snapshots, including upstream
tests/examples and their own notices, rather than a preconfigured offline build.
Native DXC source locations are supplied separately from the Rust source bundle.

The collector handles two reviewed exceptions to generic MIT detection.
`zgui-color` uses the Microsoft Terminal notice retained with the shader patch.
For `void` 1.0.2, the
[original README](https://github.com/reem/rust-void/blob/ab2f4dbb7c95c144ccba2fa8afd11e155aa91133/README.md)
declares MIT and credits its author but contains no full license file. The
document preserves that README and separately reproduces the
[standard MIT permission terms](https://spdx.org/licenses/MIT.html),
omitting the unfilled copyright placeholder rather than
inventing a holder or year. Source hashes are checked before either exception is
used. An unknown generic MIT case stops assembly and requires review.

Include all five generated files together in the distribution, plus its binary
and user instructions. Check their hashes after copying. Successful assembly
does not establish native compatibility, clean-machine runtime availability or
the final distribution font decision.

## Material outside the generated report

Keep the project license and NOTICE, this folder's upstream notices, local patch
notices, and the notices for native libraries such as DXC. Fonts and binaries
downloaded by build scripts are not fully described by Cargo package metadata.
Martian Grotesk is a user-installed development font and is not bundled.

Follow [MPL 2.0 section 3.2](https://www.mozilla.org/en-US/MPL/2.0/#distribution-of-executable-form)
when providing the corresponding source for MPL-covered code, including zgui-style's
derived traversal file and the relevant Stylo crates, together with the source
location notice. Use the exact locked revisions and include local modifications;
do not point only at an upstream moving branch. Keep generated source archives,
review reports and package outputs outside version control.
