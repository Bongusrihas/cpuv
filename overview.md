# Architecture overview

cpuv 0.0.1 is a Windows GCC environment manager with parallel downloads, compatible binary packages, recipe-backed source builds, and a managed compile-and-run interface. It keeps source directories clean while reusing verified package content across projects.

## End-to-end flow

~~~mermaid
flowchart TD
    CLI[Command and project manifest] --> Probe[GCC discovery and ABI fingerprint]
    Probe --> Provider{Compatible binary profile?}
    Provider --> Solver[Single-snapshot dependency resolver]
    Solver --> Fetch[Bounded parallel downloads and SHA-256 verification]
    Fetch --> Store[Safe extraction and BLAKE3 content store]
    Store --> Env[Staged project environment]
    Provider --> Recipes[Pinned source recipes and saved build profile]
    Recipes --> Sources[Parallel verified source downloads]
    Sources --> Native[Build with selected GCC and pinned CMake/Ninja]
    Native --> Env
    Env --> Build[Managed GCC invocation]
    Build --> Run[Execute with dependency DLL paths]
~~~

## Workspace components

| Crate | Responsibility |
| --- | --- |
| cpuv-cli | Commands, project configuration, lockfiles, source recipes/builds/tool bootstrap, synchronization, and managed run behavior |
| cpuv-toolchain | Compiler discovery, macro/configuration probes, supported language level, ABI identity, and provider selection |
| cpuv-solver | MSYS2 package metadata, dependency resolution, constraints, and similar-name suggestions |
| cpuv-fetch | Repository indexes, concurrent HTTP downloads, retries, offline behavior, and integrity checks |
| cpuv-store | Secure archive extraction, content hashing, package records, hardlinks, and copy fallback |
| cpuv-relocate | Environment indexing, compiler options, GCC specs, and relocatable package metadata |
| cpuv-shim | Compiler-shim dispatch and environment-aware GCC execution |

The CLI links the shim library. During initialization it copies its own executable into .cpuv/bin/g++.exe. Invocation under a compiler name dispatches to the shim instead of the CLI parser. Consequently a single cargo install executable can create a complete environment without a separately installed helper. The standalone cpuv-shim binary remains available for development and distribution.

## Standalone distribution

The prebuilt Windows CLI also dispatches as a native installer when named cpuv-setup.exe. Setup copies itself into the user's installation directory as cpuv.exe, updates only the user PATH through the Windows registry APIs, and broadcasts the environment change. It requires no shell, administrator account, Rust installation, or source checkout. The same operation is available as cpuv self install.

The Windows MSVC build statically links its C runtime. Release packaging produces a native setup executable, checksum files, a portable ZIP with an install.cmd entry point, and a self-contained PowerShell download bootstrap. The bootstrap verifies the downloaded setup checksum and updates its current session PATH after native installation. It never falls back to building source. GitHub releases must contain the prebuilt assets before the public installer URL works. See docs/releases.md.

cpuv self update discovers the latest stable release through GitHub's versioned release redirect, downloads bounded assets, checks SHA-256 and the candidate's reported version, and replaces only the invoked executable through the self-replace library's Windows implementation. Checks, offline requests, malformed assets, and downgrade requests are handled before replacement. Failed verification preserves the installed executable.

## Toolchain discovery and compatibility

Compiler selection follows an explicit path, CPUV_CXX, CXX, then PATH. Generated project shims are excluded from discovery to avoid recursion. The source contains no fallback tied to a particular user's profile, compiler drive, or MSYS2 installation directory.

The probe reads GCC's target, verbose configuration, predefined macros, libstdc++ configuration, accepted language standards, and linker selection. The ABI identity records compiler version, architecture, C runtime, threading, exception handling, libstdc++ release and dual ABI, target, and language ceiling. Compiler and linker paths are recorded only as detected local environment metadata.

Native compilation is distinct from binary package availability. Windows GCC distributions can compile using their own runtime and libraries even when cpuv has no matching binary provider. Source recipes build dependencies for those compilers and custom profiles. Managed prebuilts are scoped to MSYS2 UCRT64 x86-64 POSIX/SEH with libstdc++ dual ABI 1. Fingerprints are compatibility filters, never mathematical guarantees:

| Runtime | Repository namespace | Index | Archive prefix |
| --- | --- | --- | --- |
| UCRT | mingw-w64-ucrt-x86_64 | ucrt64.db | ucrt64/ |

The compiler identifies the provider; installation-directory spelling does not. Unsupported binary combinations route to source recipes rather than mixing runtimes. Missing recipes and unsupported library features fail explicitly. cpuv doctor compiles, links, and executes a native probe. cpuv does not translate object formats, C runtimes, or C++ ABIs. GCC invokes its own compatible linker, including a driver-supported alternative when available.

## Resolution and lockfiles

The resolver uses one repository database snapshot. It normalizes short package names into the selected namespace, follows dependencies and virtual providers, and checks version constraints against that snapshot. It does not search historical repository versions or pretend a rolling repository is a complete semantic-version registry.

Names are resolved exactly against the selected registry, including declared virtual providers. Missing names produce suggestions ranked from that registry. No named-library aliases or sibling-package injection are built into the solver. Package-owned metadata supplies header and link requirements; ambiguous compiled-library modules require explicit project configuration.

cpuv.lock stores exact package records, checksums, repository information, and the compiler ABI identity. cpuv sync restores that resolution. cpuv lock explicitly refreshes it. Offline operation requires the needed index and archives to be cached. Old locked packages may disappear from the upstream rolling repository, so cache retention matters.

Source mode uses a version-2 lock containing exact source/tool recipes, checksums, profile flags, and compiler observations. Local compiler/cache paths remain under .cpuv. The embedded recipes.toml supplies fmt, Eigen, and zlib; custom registries extend it. Every source dependency requires an explicit recipe and compatible compiler features.

CPPFLAGS/CFLAGS/CXXFLAGS/LDFLAGS are saved in the source lock and reused by dependencies and consumers. A compiler or profile change rebuilds the dependency environment. Unset variables retain saved flags; --reset-profile clears them before applying current values. Cache keys include compiler executable hashes, observations, recipes, tools, language mode, and generator revision. Cache hits verify output contents.

## Download and storage pipeline

The async HTTP pipeline uses a configurable transfer semaphore, defaulting to 50. Parallelism is bounded by ready packages and available network connections; it does not create fake transfers to fill the limit. Failed requests are retried, archives are written to temporary files, and their SHA-256 digests must match the index before extraction. Extraction work is independently bounded to avoid excessive CPU and disk pressure.

Shared transfer policy distinguishes index metadata from archives. Defaults allow 16 GiB archive downloads, 64 GiB expanded payloads, and one million entries per archive, with a one-hour whole-transfer deadline. Each limit is configurable. The 64 MiB compressed-index cap is independent of package sizes. HTTPS is required except parsed loopback addresses; cpuv never binds a local server port.

Archive entries are checked for traversal, unsafe links, and unexpected package prefixes. Verified file content is stored by BLAKE3 hash; package records are keyed by the archive digest. Environment construction links identical blobs where supported and copies them when hardlinks are unavailable. Files requiring relocation are rewritten as private copies so metadata updates do not mutate shared cached content.

Known host pacman metadata under usr/share/libalpm/hooks and usr/share/libalpm/scripts is accepted and discarded, including its directory ancestors. It is counted against archive limits, never executed, and never merged into the compiler prefix. This supports real GLib/Qt archives without accepting arbitrary usr/bin payload or another toolchain's files.

The --needed add mode replays a compatible existing lock for unchanged direct requirements, retaining cache verification and repair. An explicit base-devel request is delegated to pacman in the selected MSYS2 installation after project libraries synchronize. It is host state, not ABI-compatible project payload, and is intentionally not recorded as a MinGW dependency. Host pacman transactions have their own script and hook behavior and cannot be rolled back as part of a project environment swap.

Project and package locks coordinate concurrent operations. Synchronization prepares a staging environment and swaps it into place with rollback on normal errors. This is not a crash-proof transaction across every filesystem failure or power interruption.

Source mode bootstraps SHA-256-pinned CMake and Ninja into the cache, downloads source archives concurrently, then builds recipes in dependency order using parallel compiler jobs. Later dependencies see the staged install prefix and DLL directory. Interface metadata determines linking, and license files are retained. Source outputs use their compiler's runtime rather than an imposed UCRT policy. See [source builds](docs/source-builds.md).

## Environment and build execution

The generated .cpuv directory holds dependency headers, libraries, runtime DLLs, activation scripts, compiler metadata, and the compiler shim. The Windows hidden attribute keeps generated content out of the normal source-directory view. This directory is machine-local and must not be committed.

Managed cpuv run compiler commands rewrite output paths into .cpuv/build. Compile-only outputs, dependency files, and executable outputs are handled separately. Arguments after -- go to the program. The project remains the working directory; standard streams and exit codes are inherited. Failed compilation never launches an older executable. Compiler queries, shared-library builds, and preprocessing commands are not executed as programs.

Runtime lookup adds project and compiler DLL directories to PATH. GNU linker groups apply only to managed binary package plans. Native/source builds do not introduce those options automatically. GCC options, including compatible linker selection flags, continue to pass through the driver. Compiler response files are outside the managed rewriting contract in this release.

Raw compiler invocation from an activated shell follows GCC's normal output conventions. The managed run command provides the clean-output and automatic-execution behavior. Cleanup archives loose build artifacts from the source root under .cpuv/build/legacy, preserving their contents, source files, and project configuration.

## Trust boundaries and limitations

- Repository indexes and downloaded archives are untrusted input. Paths and checksums are validated before materialization.
- SHA-256 verifies agreement with the index; it does not authenticate a compromised index. Package signature verification is not implemented.
- Project-library package installation scripts and pacman hooks are not executed. Explicit host-tool requests use MSYS2 pacman normally.
- Provider and ABI checks cannot prove all upstream library build options are compatible.
- Source builds execute trusted upstream build code; they are not sandboxed or hermetic. Compiler features, recipe availability, and driver/linker compatibility still constrain support.
- Arbitrary compiler-family interoperability and non-Windows hosts are outside version 0.0.1.
- Hardlinked cache content is shared and should be treated as immutable.

## Source distribution

All first-party implementation crates are included, together with Cargo manifests, the dependency lockfile, build/package/install scripts, license notices, and documentation. No private cpuv service or undistributed implementation component is required to build the executable. Third-party Rust dependencies are obtained through Cargo.

This source distribution excludes test suites, development simulations, benchmark scripts/results, and generated reports. The Windows CI workflow checks formatting, Clippy, and compilation. Release packaging produces the native installer and portable executable; prebuilt users do not require Rust.

Package linking follows selected pkg-config modules and their public Requires graph; neither source includes nor archive enumeration chooses libraries. Environments include a PE/COFF and packaged-symbol compatibility report. Specs remain an explicitly loaded experiment. See [architecture](docs/architecture.md) and the [compatibility policy](docs/compatibility.md).

## Ownership

Author: **Bongu srihas**. Co-authors: **Gemini 4 argon** and **GPT-6 sol**. Version: **0.0.1**. Project code is Apache-2.0; upstream packages retain their own licensing.
