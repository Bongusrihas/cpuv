# Phase 2: Windows development workflows in 0.0.5

These workflows are included in 0.0.5. The gates record the compiler and consumer combinations they exercise.

## Local verification boundary

| Compiler/provider | Exercised workflow |
| --- | --- |
| GCC 16.2.0 UCRT64 x64 | Pinned source inputs, SDK declarations, static Release and shared Debug profiles, CMake/CTest, Make, offline transfers and cache recovery. |
| MSVC 19.51 x64 | MSBuild source package and C++20 consumer, CMake/CTest, plugins, resources and delay-import deployment. |
| GNU Clang 22.1.8 x64, libc++/UCRT | Native consumers, response files, shim, CMake/CTest and runtime deployment. |
| Stable clang-cl 23.1.4 x64, Microsoft SDK | Native consumers, response files, shim, Conan library build, frozen/offline restore, CMake/CTest and deployment. |
| GCC 16.2.0 x86, MSVCRT/DWARF2 | Native execution on x64 Windows, shim, CMake/CTest, deployment and linker/LTO probes. |
| LLVM-MinGW 23.1.4 ARM64 target | Verified cross-compiler provisioning and compile/link probe; execution is not tested on this x64 host. |
| vcpkg/MSVC x64-windows | Locked graph, CMake/CTest, offline missing-tree repair and runtime deployment. |

These are local integration cases, not certification of unlisted compiler versions or Windows images. Fresh Windows Server 2022/2025 CI lanes exercise the retained gate when this source is pushed. Windows desktop images, native ARM64 execution and the historical compiler/library matrix remain Phase 3 coverage.

## Source packages and discovery

An absent catalog recipe used to end automatic installation even when the upstream source could build on Windows. CPUv now accepts project-owned declarations for checksummed archives, immutable Git commits, hashed local trees, and prebuilt SDKs. The engine selects a generic CMake, Meson, Make, MSBuild or copy adapter. No library name selects a recipe, linker flag or patch.

```powershell
cpuv search --catalog msys2 library
cpuv search --catalog conan library
cpuv search --catalog vcpkg library
cpuv source inspect .\vendor\library
cpuv source add library .\library-source.toml
cpuv plan
```

The declaration file describes one dependency, for example:

```toml
version = "1.2.3"
linkage = "static"
[source.input]
kind = "local"
path = "vendor/library"
sha256 = "REPLACE_WITH_THE_SNAPSHOT_HASH_FROM_SOURCE_INSPECT"
[source.build]
system = "cmake"
variables = { BUILD_TESTING = "OFF" }
[source.interface]
libraries = ["library"]
cmake_file = "library"
cmake_target = "library::library"
```

Replace the paths, hash and public interface with the upstream project's actual values. For archives use `kind="archive"`, HTTPS `url`, `sha256`, `format="zip"`, `"tar.gz"` or `"tar.zst"`, and `strip_components`. For Git use `kind="git"`, HTTPS or a project-relative repository `url`, a full 40-digit `revision`, and optional `submodules=true`. Git hooks and executable transports are disabled. A cache miss for a remote Git input fails offline. Local inputs exclude `.git` and `.cpuv`, reject links/reparse points, and include file names and contents in their snapshot hash. Changing an available local tree is rejected until its declaration is explicitly updated.

`source.requires` contains declared dependency names or Conan references. `source.build.tools` contains upstream Conan tool requirements. The upstream library still needs a compatible compiler, its dependencies and an accurate interface. Native CMake imported targets remain the preferred consumer interface when available.

SDK declarations use `kind="sdk"`, project-relative `path`, `sha256`, the observed `target` and `runtime`, and the `copy` build system. They must match the selected compiler ABI. `redistributable=true` is required before a development export includes an SDK. CPUv does not infer licensing permission or transform incompatible binaries. Vcpkg and declarative source packages use separate profiles.

For Make/MSBuild packages declare `source.build.install` as source-to-package mappings; source keys can begin with `source/` or `build/`. MSBuild also requires `source.build.project`. For copy packages the complete immutable source tree becomes the package prefix. `license_files` lists source-relative licenses. Interface fields include `libraries`, `system_libraries`, `defines`, `include_dirs`, `library_dirs`, `cmake_file` and `cmake_target`. Compiler/profile overrides are rejected in build variables and CMake configuration arguments.

## Compilers, specialist tools and capabilities

```powershell
$env:CPUV_MAX_ZSTD_WINDOW_MIB = '1024'
cpuv toolchain provision .\examples\toolchains\llvm-cl-23.1.4.toml
cpuv --compiler 'C:\tools\llvm\bin\clang-cl.exe' toolchain capabilities
```

A compiler manifest records a name, target, exact version, archive URL/SHA-256/format, strip count and executable path. Provisioning creates an independently cached installation, checks the actual target/version, then compiles/links and runs a probe when the host can execute the target. `--use` selects it without editing Windows PATH. Licensed Microsoft SDK/Build Tools prerequisites must already be installed. `capabilities` exercises LTO and applicable alternative linkers; an unavailable linker produces an explicit unsupported result with diagnostics.

Target and build-tool profiles have separate identities. New identities depend on compiler/SDK contents rather than installation locations. Existing provider identity schemas remain readable for frozen legacy locks. Native x86 on x64 can execute; ARM64 on x64 is recorded as compile/link only. Probe success is narrower than complete dependency support.

Project `[tools.NAME]` entries use the same verified artifact fields: `version`, `executable`, `url`, `sha256`, `archive`, and `strip_components`. Optional `interpreter` names another pinned tool. `conan_package` advertises a supplied tool to Conan only after its real version passes `version_arguments` (default `--version`). No system Python is needed for the retained Meson fixture: it declares a standalone Python and Meson archive. Python bytecode writes are disabled to keep these installations immutable. Changed tool trees are quarantined and reconstructed from verified archives.

## Builds and process controls

`[build].system` selects `cmake` (default), `meson`, `make`, or `msbuild` for `cpuv build` and `cpuv test`. `variables` supplies build variables and `arguments` supplies configuration/build arguments. CMake and Meson tests accept `test_arguments`; Make defaults to the `test` target; MSBuild requires explicit `test_arguments`, such as `["/t:RunTests"]`, and a `project` file. Use `[tools]` for project tools; `build.tools` and `build.install` are source-package fields.

Make and MSBuild work on independent source copies under `.cpuv/build`; generated compiler flags and MSBuild properties bind the selected profile. CMake and Meson use separate build directories. Build-stage logs are retained under `.cpuv/logs`; provider logs remain in the shared cache. Windows Job Objects assign suspended processes before resuming them, enforce optional resource limits, and terminate descendants on cancellation or timeout.

| Setting | Default | Meaning |
| --- | --- | --- |
| `CPUV_PROCESS_TIMEOUT_SECS` | 3600 | Per managed provider/build command; 0 disables the deadline. |
| `CPUV_MAX_LOG_MIB` | 64 | Maximum retained log size; overflow stops the process tree. |
| `CPUV_MAX_PROCESSES` | 256 | Simultaneous processes in the managed job; 0 disables this cap. |
| `CPUV_MAX_PROCESS_MEMORY_MIB` | 0 | Optional total job memory limit. |
| `CPUV_MAX_ZSTD_WINDOW_MIB` | 128 | Decoder window; powers of two from 32 to 2048. The LLVM example needs 1024. |

The compiler guard distinguishes libstdc++ assertion checks from ABI-changing debug-container/dual-ABI settings. `_GLIBCXX_ASSERTIONS` can be enabled per translation unit without rebuilding dependencies; [GCC documents this ABI boundary](https://gcc.gnu.org/gcc-6/changes.html).

These controls cover CPUv-managed build/provider commands. They are not a security sandbox and do not restrict arbitrary recipe networking. Interactive `cpuv run` retains its normal terminal behavior.

## Locks and recovery

New `cpuv.lock` files use a version-5 envelope around their native provider payload. The envelope records the provider, request hash and payload digest. Legacy versions 1–4 are readable; migration and explicit graph changes preserve content-addressed backups under `.cpuv/lock-backups`. Frozen Conan restores preserve the upstream lock bytes so export timestamps cannot silently rewrite a lock.

```powershell
cpuv generation list
cpuv generation use GENERATION_ID
cpuv --offline sync
```

Successful changes retain independently copied, verified generations under `.cpuv/generations`. A generation contains the installed request, lock, environment, compiler state and activation/shim files. Restoring verifies its inventory and compiler/SDK contents and uses the recoverable transaction. It applies at the original project location; use development export/import for moving a project. A new compiler or intentional request change requires `cpuv lock`.

Provider-independent immutable deployed artifacts repair a missing environment offline, including vcpkg, when the exact request, lock and compiler/SDK contents match. This does not promise that every provider can rebuild an uncached dependency without its original sources/tools.

## Runtime deployment

`[runtime]` declares project-relative `plugins`, `assets` and `search_paths`. `[runtime.environment]` supplies child-only variables; `${root}` expands to the project or deployed application root and `${path}` to the inherited runtime search path. Plan asset paths so they also match the deployment layout: file assets are copied by filename and directory assets retain their final directory name.

The bundle follows ordinary PE imports and supported RVA/legacy-VA delay-import descriptors. Explicit PE plugins are scanned even with extensions such as `.pyd`. Missing DLLs, conflicting contents and machine mismatches fail before publishing. Environment declarations generate a digest-recorded `cpuv-run.ps1` launcher. Run this launcher when the application needs those variables. An application using dynamic loading must still declare its plugin/data assets.

## Offline development transfer

```powershell
cpuv offline export D:\transfers\project
cpuv offline inspect D:\transfers\project
cpuv --offline --cache-dir D:\offline-cache --compiler C:\tools\gcc\bin\g++.exe offline import D:\transfers\project D:\work\project
cpuv --project D:\work\project --cache-dir D:\offline-cache --offline sync
cpuv --project D:\work\project --cache-dir D:\offline-cache --offline test
```

Export includes the project source snapshot, locked deployed dependency binaries, selected pinned build tools, source snapshots and integrity inventories. MSYS2 exports include the selected verified package archives so its normal cache quarantine and reconstruction remain active after import. Conan exports/imports its selected graph through its public `list`, `cache save` and `cache restore` commands; authentication files and raw home databases are not copied. Package-source rebuilding additionally requires the provider's full source/tool closure. The development guarantee is building consumers and restoring the exported locked deployment offline.

Import requires a new project directory and verifies the transfer, compiler/SDK prerequisite identity and cache collisions. Generated CMake/pkg-config paths are rebased to the destination. Existing project directories are preserved. By default compilers/SDKs are external prerequisites, recorded in the receipt. An optional `--toolchain-root PATH` exports a complete redistributable GNU compiler tree; Microsoft compiler/SDK trees remain external. Preserve upstream redistribution terms. Select an explicit compiler when importing with `--compiler`; absent that option a bundled compiler can be used.

## Cache inspection and collection

```powershell
cpuv cache inspect
cpuv cache prune
cpuv cache prune --quota-mib 10240
cpuv cache prune --quota-mib 10240 --apply
cpuv cache forget D:\intentionally-removed-project
```

Pruning is a preview until `--apply`. Registered projects, their saved generations, incomplete recovery state and provisioned compiler receipts protect referenced immutable objects. Missing registered projects conservatively block collection until explicitly forgotten. Shared cache leases keep collection from racing CPUv operations. Only CPUv immutable environment/source/tool objects with valid receipts and verified contents are eligible; a hash-shaped directory name alone does not establish ownership. Unowned or damaged objects, provider databases, store objects, archives, quarantine and project recovery backups remain retained. Quotas reclaim eligible objects oldest first and report remaining usage; protected content can exceed a quota. Provider-owned cache maintenance remains with its provider.

## Acceptance

`scripts/test-phase2.ps1 -Compiler C:\path\to\ucrt64\bin\g++.exe` runs workspace checks, the native source/lock/generation/offline/cache fixture, Meson/MSBuild/runtime adapters, stable clang-cl consumer integration, an ARM64 compile/link-only toolchain and linker/LTO probes. Optional `-AdditionalCompiler` entries run native consumer/CMake/bundle checks. `-CrossCompiler` entries record compile/link-only probes where execution is unavailable. `-ReleaseBuild` uses optimized binaries.

The gate retains source hashes, compiler evidence and logs. CI configures fresh Windows Server 2022/2025 lanes, and release packaging requires both Phase 1 and Phase 2 gates. These workflows must run on the changed source before reporting their remote results. Native ARM64 execution, historical compiler catalogs, every Windows 10/11/Server image and broad library coverage remain the Phase 3 matrix; untested combinations stay experimental.

## Corrected failures and their causes

| Failure | Cause | Correction |
| --- | --- | --- |
| Local Git repositories with Windows long paths looked like remote hosts. | Git received a verbatim Windows path with its special prefix. | Normalize the driver path before passing the local repository URL and verify the checked-out commit. |
| Frozen restores rewrote a lock after exporting an unchanged recipe. | Conan refreshed export timestamps in its output lock. | Verify the restored package graph and retain the original upstream lock bytes for frozen sync. |
| A restored environment failed transaction validation. | New staging directory names did not follow the recovery journal's accepted naming convention. | Use the existing stage convention and recoverable commit path. |
| A saved generation could contain an edited, uninstalled request. | History copied the live manifest after the user changed it. | Snapshot the sealed environment's installed request instead. |
| Meson worked once and failed offline integrity checks. | Python wrote bytecode into pinned tool trees, and PATH could select another Ninja. | Disable bytecode writes, explicitly select the pinned Ninja, quarantine changed tool trees and restore verified archives. |
| Meson assertions were rejected as ABI changes. | The source guard classified every libstdc++ macro as ABI-changing. | Permit the documented assertion-only macros while retaining checks for debug containers, dual ABI and unknown macros. |
| MSBuild chose old tools or could not find a dependency header. | A project defaulted to v100, and an empty flag request was interpreted as a compiler query. | Bind the selected toolset/compiler/SDK/linker and request actual compilation flags for the imported property sheet. Toolset names follow the observed STL release, not compiler-minor arithmetic. |
| Server CI could not build an older MSVC toolset installed in a newer Visual Studio. | Conan inferred the IDE version from the compiler version and searched for an absent older installation. | Query the selected installation with `vswhere -path` and explicitly pass its path and IDE version to Conan; preserve the selected compiler/toolset. |
| clang-cl catalog builds requested absent `14.3` tools on a machine with `14.4` tools. | Conan's activation selectors and MSBuild platform names differ: its `v144` selects `14.4`, while MSBuild calls that platform `v143`. | Derive the Conan selector and exact update suffix from the selected toolset directory; keep the observed STL release for the MSBuild platform name. |
| New profiles could switch a Conan project to automatic provider selection. | Creation preserved the vcpkg backend but omitted the original Conan backend. | Preserve the original backend/settings when copying a request into a named profile. |
| Runtime export produced an unusable launcher. | An operating-system string serialized as a platform object instead of an executable name. | Serialize a validated Unicode filename and include the generated launcher in the digest receipt. |
| A source package installed successfully but its program failed with a missing-DLL status. | The upstream install omitted a private shared dependency. | Check deployed runtime imports before commit; declare missing runtime outputs through generic install mappings or provider dependencies. |
| Offline snapshots could collide across different compiler installations. | The cache key used the lock and request without the compiler's contents. | Include the portable compiler/SDK digest, and retain distinct provisioning receipts for different target drivers in one archive. |
| Imported metadata still depended on old paths or rewrote a destination twice. | Simple sequential substitutions omitted compiler paths and could rewrite newly inserted text. | Rebase recorded compiler/SDK/cache/environment prefixes in one substitution pass. |
| Snapshot restore bypassed shared MSYS2 corruption recovery. | The generic offline shortcut restored a project without visiting its damaged shared blobs. | Keep MSYS2 on its normal quarantine/reconstruction path and export the pinned archives needed for that path. |
| Cache collection could select an unrelated hash-shaped directory. | The initial ownership check relied on its location and name. | Require CPUv receipts and verified immutable contents before collection; retain unowned or damaged entries. |

These are generic path, profile, integrity, adapter and runtime corrections. They do not add library-name branches or convert an incompatible upstream binary into a compatible one.

Conan documents separate [Visual Studio installation and IDE version settings](https://docs.conan.io/2/reference/config_files/global_conf.html) for toolsets installed in a different IDE release.
