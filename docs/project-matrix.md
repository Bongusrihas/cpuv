# Live Windows project matrix

The matrix selects distinct upstream GitHub projects from the MSYS2 UCRT64 package index. A fixed hash order makes the selection repeatable from a saved index. The sample limits the compressed and expanded binary dependency graphs so the matrix can run on a workstation.

The source archive, Git clone and package modes measure separate operations. The archive mode downloads a commit-stamped upstream ZIP and verifies its CRCs. Git mode fetches a shallow repository and records its commit. Package mode runs `cpuv init` and `cpuv add` in separate folders, checks the SHA-256 of the locked package archive, runs offline `verify` and compiles and runs an Expat and zlib consumer. When automatic linking fails, it records the error and tries the same install with the two modules explicitly selected.

Run these commands from the repository root with Python 3.11 or later, Git, curl and UCRT64 GCC installed.

```powershell
curl.exe -fL https://repo.msys2.org/mingw/ucrt64/ucrt64.db -o ucrt64.db
python scripts/test-project-matrix.py --mode archive --root tmp/matrix --database ucrt64.db --count 200
python scripts/test-project-matrix.py --mode git --root tmp/matrix --database ucrt64.db --count 200
python scripts/test-project-matrix.py --mode package --root tmp/matrix --database ucrt64.db --count 200 --binary target/release/cpuv.exe --compiler C:/msys64/ucrt64/bin/g++.exe --adjustments scripts/fixtures/project-matrix-adjustments.toml
```

The generated case folders contain the CPUV manifest, lock, source or archive evidence, command logs and JSON result for each tested operation. A custom `--repository` selects an HTTPS UCRT64 mirror for the package run. The installed versions and checksums come from that mirror's index and can differ from the sampling index as mirrors update.

## November 4, 2026 findings

On Windows x64 with UCRT64 GCC 16.2.0, the first source pass exposed DNS failures and slow transfers. The Git retry downloaded 199 of 200 shallow repositories, and a separate retry completed xsimd after its port 443 connection failed. The ZIP test verified 217 of 220 archives. Three exceeded the retry's 160 MiB compressed size limit. All three downloaded through Git, so every sampled upstream repository had a verified source download by at least one route.

| Finding | Cause and correction |
| --- | --- |
| Slow MSYS2 archive downloads | The primary host timed out partway through a transfer that completed via an official mirror. CPUV now uses the mirror service for an archive retry and checks the same length and SHA-256 from the original index. |
| Libraries installed for later use blocked `add` | `is_utf8`, `hlsl2glsl` and `libpaper` lacked an unambiguous automatic module. Brotli exported three modules. CPUV now installs these packages and reports which ones need an explicit link choice. |
| Warm installs copied the full environment | Online MSYS2 installs published a duplicate deployed snapshot even though the verified archives and CAS could restore the lock. CPUV now makes the independent snapshot on offline sync or export. Reusing an unchanged environment also skips a generation copy. The same 16-package warm graph took 196 seconds before the change and 9 seconds after it on this workstation. |
| Draco and xlnt declared `-Iinclude -Llib` | CPUV resolved relative directories from the current working directory. It now anchors them to the managed environment and checks that the resolved paths remain inside it. |
| M4RI embedded an MSYS2 CI path | Its `.pc` file named `D:/M/msys64/ucrt64/include/libpng16`. CPUV can rebase recognized MSYS2 install paths when the matching directory is inside the managed environment. |
| LSP DSP advertised `-llsp-dsp-lib` | The package owns `lsp-dsp-lib.a`, a filename GNU ld cannot find through that spelling. CPUV uses an exact filename operand for an owned archive when a conventional link filename is unavailable. |
| LCOV failed while installing Perl | The archive includes a host startup script under `etc/profile.d` and a Perl data file named `cFile.pc`. CPUV skips the host script and preserves the data file. It reads pkg-config modules from module locations and still rejects unrecognized archive paths outside the toolchain prefix. |

The catalog also contained requests that needed a project change. With the [adjustment fixture](../scripts/fixtures/project-matrix-adjustments.toml), the test runner records each initial error and submits the specified project request. `zlib-ng-compat` replaces zlib in its project because the two packages conflict. `hidapitester` needs `hidapi`, `kiss_fft` needs `libpng` for a packaged executable, `opencl-icd` needs `opencl-headers` for its pkg-config interface, and `libmongoose` needs `openssl` for the `libssl` module. These remain explicit project dependencies until the upstream metadata declares them. An LSP DSP API consumer also passed with `-DLSP_DSP_LIB_BUILTIN`, which selects its static header interface.

## Ten installed, two used

The ten direct packages were zlib, Expat, xxHash, libdeflate, bzip2, XZ, Zstandard, LZ4, libiconv and Brotli. The lock recorded 16 packages including transitive runtime dependencies. zlib and Expat had no dependency edges to each other or to the other eight direct packages in that graph.

The default link plan listed the unambiguous modules, and the explicit plan selected only zlib and Expat. The executable called both libraries and imported their DLLs. The other eight libraries remained installed. The explicit bundle included the imported runtime DLLs and ran with only the Windows system directory on `PATH`. A separate program in the same ten-package setup called APIs from the other eight libraries. Repeating `add --needed` kept the lock, and an impossible version request left it intact.

Additional consumers exercised yaml-cpp parsing, Snappy compression, cmark rendering and a yaml-cpp imported target under CMake/CTest. CPUV also built zlib from a pinned Git commit and a SHA-256 verified ZIP, then verified, ran and restored each project offline.

## Release candidate results

All 200 sampled package projects ultimately installed, verified and compiled and ran the two-library consumer. The first pass produced 195 passing projects. After the link and project corrections, the next pass produced 199. The final binary passed the remaining LCOV case and preserved Perl's `cFile.pc` byte for byte. The 199 earlier passing projects contained no `.pc` files outside module locations. LCOV reported version 1.16 with a warning that its Perl runtime fell back from the inherited `C.UTF-8` locale to the Windows system locale.

| Check | Result |
| --- | --- |
| Upstream downloads | 200 shallow Git clones after retry. All 220 sampled repositories obtained through Git or a verified ZIP. |
| CPUV package projects | 200 of 200 passed after corrections. Each sampled locked package archive passed SHA-256 verification. Five cases needed an explicit provider dependency or replacement. |
| Final optimized Rust workspace tests | 76 passed, 0 failed, 5 ignored. The ignored tests are the three native gates, a crash subprocess helper and a manual benchmark. |
| Native release gates | GCC Phase 1, Phase 2 transfers, and MSVC Phase 3 passed at the recorded candidate checkpoints. The final module-location fix was checked by the workspace suite and LCOV follow-up. |
| Source and packaging checks | Formatting, Clippy, Rust 1.88 compatibility, RustSec audit and isolated 0.0.5 setup passed. The audit reported zero vulnerabilities and warnings. |

The local report, source and executable hashes, retained projects, installation logs and package results are under `tmp/bulk-005`.

The final publication check passed 77 optimized workspace tests. A six-project follow-up verified the compiler build base in a live lock, replayed it offline and exercised all five adjustments from the external fixture.
