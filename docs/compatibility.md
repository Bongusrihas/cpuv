# Compiler, linker, and ABI compatibility

Version **0.0.4** targets native Windows C/C++ workflows. Compiler, runtime, SDK, architecture, library options and upstream package support must agree; downloading a library does not convert its ABI.

## Application platform

The prebuilt cpuv executable targets Windows 10 or newer on x86-64 and statically links its Microsoft C runtime. It needs no Rust/Cargo or separate Visual C++ runtime installation. Building cpuv requires Rust and its platform tools. Compiling applications requires a working selected compiler. MSYS2 compiler installation uses an existing pacman host; cpuv does not bootstrap MSYS2. The vcpkg adapter requires Git and Microsoft C++ Build Tools and bootstraps provider-declared tools into its cache.

## Tested lanes and boundaries

| Lane | Evidence and boundary |
| --- | --- |
| UCRT64 GCC 16.2.0, x64 | Main Phase 1 acceptance lane: native static/shared consumers, ordered and cyclic archives, shim/runtime deployment, frozen/offline restore, corruption repair, crash recovery and installer contracts. |
| MSYS2 binary packages | UCRT64/UCRT or MINGW64/MSVCRT; x64 POSIX threads, SEH and libstdc++ dual ABI 1; matching repository/runtime packages required. |
| Conan | Compiler/profile-specific upstream recipes, generic options and verified shared/static selection. Complete cached binaries can be redeployed offline. Recipes control source compatibility and may need additional tools. |
| MSVC 19.51 x64 | Discovery and toolset environment, native compilation/shim/response files, Conan source consumer, CMake/CTest, offline restoration and runtime bundle exercised during implementation. Broader toolset/Debug/shared coverage remains incomplete. |
| GNU-style Clang 22.1.8, libc++/UCRT x64 | Native consumer, response files and Conan zlib consumer exercised. Upstream packages may still fail against the installed headers; no package-specific engine workarounds are applied. |
| vcpkg with MSVC x64 | Pinned registry/tool revision, native versions/features, `x64-windows` CMake/CTest and runtime export exercised. `x64-windows-static-md` is accepted but experimental. Direct `cpuv run cl` dependency injection is unavailable; use CMake targets. Offline mode verifies an intact installation, without rebuilding a missing tree. |
| clang-cl / additional architectures | Adapter groundwork exists; complete native acceptance remains open. Do not infer support from discovery alone. |
| Cygwin / WSL / cross-OS binaries | Separate execution environments; outside the native Windows adapters. |

These checks establish specific consumers on the test host. Clean Windows image coverage, old/current compiler matrices, complete plugin/resource coverage and every library/version combination are not certified. See [0.0.4 verification](release-notes/v0.0.4.md).

## Selection, profiles and restoration

Compiler discovery considers an explicit executable, CPUV_CXX/CXX, a saved selection, PATH and discovered installations. Project operations preserve their recorded compiler unless explicitly changed. Named profiles own separate manifests, locks and environments; later root-manifest changes are not automatically propagated to every profile. Release/Debug and runtime/options are part of the request.

`cpuv sync` restores a compatible selected lock; a missing lock or changed request/profile is rejected. Use `cpuv lock` to resolve intentional changes. `--refresh` does not turn sync into a graph update. Changed flags must be applied with an explicit lock update; `--reset-profile` clears saved flags before applying current settings. Builds and the standalone shim reject altered environment contents; sync can restore supported complete caches. No missing offline input is silently replaced.

## ABI and linking

C++ compatibility includes object layout, symbol naming, exceptions, standard library, CRT, SDK, configuration and feature flags. GCC-compatible archives, MSVC libraries and unrelated runtimes are not interchangeable. PE/COFF inspection, architecture/runtime guards, compiler/SDK hashes and package metadata narrow compatibility; they cannot prove every interface, CPU instruction requirement or LTO combination. See [GCC ABI policy](https://gcc.gnu.org/onlinedocs/libstdc++/manual/abi.html) and [MSYS2 environments](https://www.msys2.org/docs/environments/).

The selected compiler driver invokes its matching linker. Supported driver options can request compatible alternatives; an arbitrary linker on PATH is not a substitute. GNU plans preserve operand order, private static requirements and scoped flags. CMake evaluates provider imported targets for CMake projects. Ambiguous/missing interfaces require explicit configuration rather than library-name guesses.

## Runtime, offline and trust

Application bundles stage available runtime imports, declared assets, licenses and provenance into a new directory. Unsupported delay imports are rejected. Explicit plugin/data assets remain necessary; this is not a complete offline development bundle. Minimal-PATH execution is tested locally; clean-machine execution is additional evidence, not implied.

Conan offline flags disable provider remotes but do not sandbox arbitrary recipe subprocesses. Checksums verify content agreement; package signatures and hermetic builds are not implemented. Recipes and upstream builds execute with the user's permissions. See [security](../SECURITY.md), [catalog installation](source-builds.md), [providers](windows-providers.md) and [release instructions](releases.md).
