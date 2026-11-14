# Static DXC notices

`mach-dxcompiler-rs` downloads and links a prebuilt C/C++ compiler library. Its
Rust crate's MIT license does not describe all of the code in that library.
Keep these notice files alongside the Rust dependency notices in a distribution.

The locked crate version is `0.1.7+2026.10.26-141dd7f.1`. The native library comes
from [mach-dxcompiler release 2026.10.26+141dd7f.1](https://github.com/DouglasDwyer/mach-dxcompiler/releases/tag/2026.10.26%2B141dd7f.1),
commit `141dd7f32da0edeeb5afdfc7765519448cf98948`. Its
[build.zig](https://github.com/DouglasDwyer/mach-dxcompiler/blob/141dd7f32da0edeeb5afdfc7765519448cf98948/build.zig)
pins the compiler source to `4190bb0c90d374c6b4d0b0f2c7b45b604eda24b6`.
The original `hexops/DirectXShaderCompiler` repository redirects to
`hexops-graveyard/DirectXShaderCompiler`; the commit is unchanged.

The Windows x64 archive with dynamic CRT linkage has SHA-256
`abe009a4abb88dab9a1659ca12515e35baa34478a2ed42103595ebf65f8f6b25`.
It contains `machdxcompiler.lib`; its licenses are supplied here separately.
The crate verifies this same hash during its build. Changing the target, CRT
linkage or locked crate version requires checking the corresponding artifact.

- `mach-LICENSE`, `mach-LICENSE-APACHE`, and `mach-LICENSE-MIT` are unchanged
  copies from the wrapper/build repository at the commit above.
- `compiler-LICENSE.TXT` and `compiler-ThirdPartyNotices.txt` are unchanged copies
  from the pinned compiler source, including its embedded third-party terms.
- `sources.json` records the source URL and SHA-256 of each copied text.

When updating DXC, trace the build's compiler revision again, compare its build
options and bundled dependencies, refresh the notice texts, and verify both the
native archive hash and every notice hash. Do not infer the compiler's license
from the Rust binding crate's license field.
