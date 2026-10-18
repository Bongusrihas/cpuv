# cpuv

A clean C++ package and run workflow for Windows GCC.

**Version 0.0.2 · Windows · Apache-2.0**

cpuv manages native C++ dependencies, prepares a project environment, and compiles and runs your code with one command. Downloaded packages live in a shared cache. Generated programs and compiler artifacts stay in the hidden .cpuv directory.

## Highlights

- Parallel package downloads, with a configurable limit of 50 transfers by default.
- SHA-256 archive verification, retries, an offline cache, and a BLAKE3 content-addressed store.
- Hardlink-based project environments, with a copy fallback.
- Automatic compiler discovery and an ABI fingerprint for each environment.
- One managed binary ecosystem: MSYS2 UCRT64, selected from the detected compiler.
- Automatic ConanCenter fallback for missing MSYS2 packages and other native GCC profiles, without cpuv library recipes.
- Automatic compilation and execution, without EXE or DLL files scattered across your source directory.
- Exact package names from upstream catalogs, with no cpuv library aliases or named linker defaults.

## Requirements

- Windows 10 or newer (x64) and a native GNU C++ compiler: g++.exe must be on PATH, or selected explicitly.
- Internet access for the first dependency download. Cached dependencies can be used offline.

Installing and using the prebuilt application requires no Rust, Cargo, Git, or Visual C++ runtime installation. GCC is needed to compile your C++ programs, not to install cpuv. The current release executable targets 64-bit x86 Windows.

Compatible MSYS2 UCRT64 compilers can use prebuilt packages. Missing packages, other supported native MinGW GCC profiles, and custom ABI flags use ConanCenter automatically. cpuv bootstraps standalone Conan, CMake, and Ninja; this does not require a system Python or Rust installation. Upstream catalog metadata supplies dependencies, build steps, and interfaces. A package must exist in that catalog and support the selected compiler. vcpkg, arbitrary GitHub archives, MSVC, and Clang adapters are outside this release.

## Install

Download [cpuv-setup.exe](https://github.com/Bongusrihas/cpuv/releases/latest/download/cpuv-setup.exe) and double-click it. It installs for the current user and adds cpuv to PATH. Open a new terminal and start using cpuv in your project folder. No administrator account or build tools are required.

Or paste one command into an existing PowerShell window:

~~~powershell
irm https://github.com/Bongusrihas/cpuv/releases/latest/download/install.ps1 | iex
~~~

From CMD, or to launch PowerShell explicitly, use:

~~~console
powershell -ExecutionPolicy ByPass -c "irm https://github.com/Bongusrihas/cpuv/releases/latest/download/install.ps1 | iex"
~~~

The command downloads a prebuilt installer, checks its SHA-256 digest, installs it, and makes cpuv available in the same PowerShell window. It never compiles cpuv or downloads Rust. Files are installed under the current user's .local/bin directory; no username or drive is embedded in the source.

Release downloads include the native installer, portable ZIP, SHA-256 checksums, and PowerShell bootstrap. See [release instructions](docs/releases.md).

For an offline or portable installation, download [the Windows ZIP](https://github.com/Bongusrihas/cpuv/releases/latest/download/cpuv-x86_64-pc-windows-msvc.zip), extract it, and double-click install.cmd. Or run cpuv.exe directly from the extracted folder without installing it. Both options work without PowerShell.

Update an installed copy with:

~~~powershell
cpuv self update
cpuv self update --check
~~~

Uninstall the cpuv application with:

~~~powershell
cpuv uninstall --dry-run
cpuv uninstall
~~~

The command removes the invoked executable, matching copies in cpuv's standard per-user install locations, and recognized cpuv helpers. Dedicated installation-directory entries are removed from the user PATH. Shared directories such as .local/bin stay on PATH when other files/tools remain. Project source, .cpuv environments, GCC, and dependency caches are preserved. A custom or portable invocation affects that installation only; different binaries in other locations are reported and preserved. Open a new terminal afterward. Use cpuv rm PACKAGE to remove a project dependency instead.

Self update downloads the latest published stable GitHub release, verifies its checksum and executable version, and safely replaces the invoked Windows executable. It requires no Rust or PowerShell. Use --version 0.0.2 to select a release, or --force to reinstall the same version or explicitly request a downgrade. Other copies of cpuv on disk are not updated. Project environments and dependency caches are preserved. A separate compiler-shim download is unnecessary.

For a custom location, set CPUV_INSTALL_DIR before the one-command install, or run the downloaded setup executable with options:

~~~powershell
.\cpuv-setup.exe --install-dir "$HOME\tools\cpuv" --no-path
~~~

## Your first program

~~~powershell
mkdir hello-cpp
cd hello-cpp
cpuv init
~~~

Create main.cpp:

~~~cpp
#include <iostream>

int main() {
    std::cout << "Hello from cpuv!\n";
}
~~~

Compile and execute it:

~~~powershell
cpuv run g++ main.cpp
~~~

This works from any project folder, including paths containing spaces. cpuv runs the program with the project as its working directory, so relative data-file paths continue to work. Executables and intermediate compiler files go to .cpuv/build. Runtime DLLs are found through the environment PATH.

## Add dependencies

~~~powershell
cpuv add fmt
cpuv add 'fmt>=12'
cpuv add xtensor
cpuv add lapacke
cpuv add --needed qt6-base
cpuv add --needed base-devel qt6-base
cpuv run g++ solver.cpp
cpuv rm fmt
~~~

Use exact names from the selected registry. No library aliases or automatically added sibling libraries are built into the resolver. For example, MSYS2 calls its Eigen package eigen3, while ConanCenter calls it eigen. Request lapacke explicitly for lapacke.h; requesting lapack alone installs only that package and its declared dependencies. If any dependency is absent from MSYS2, cpuv resolves the entire environment through ConanCenter. Catalog names can differ between ecosystems; use exact names, or select ConanCenter explicitly with --catalog. cpuv does not mix the two binary graphs. Linking follows package-owned pkg-config metadata and its public Requires graph. Source include names do not select libraries, and archives are not linked just because they are present.

For multi-module packages or an explicit application interface, configure cpuv.toml:

~~~toml
[link]
pkg_config = ["Qt6Widgets", "fmt"]
# Optional additional libraries and driver flags:
# libraries = ["ws2_32"]
# flags = ["-pthread"]
~~~

Explicit selections replace automatic module selection. Automatic selection uses a module owned by the direct package whose name matches the package, or its sole exported module. A package with ambiguous or missing compiled-library exports requires explicit configuration; there are no library-specific defaults. Public/shared pkg-config interfaces are supported; static/private interfaces and arbitrary CMake target evaluation are not silently guessed. Run cpuv sync after changing link configuration. See .cpuv/env/link-plan.json for the actual selected modules and libraries. Older locks retain their recorded graph until cpuv lock resolves it again.

If an older manifest or lock relied on a removed alias or automatically added sibling library, select the exact package names and run cpuv lock to regenerate the resolution. Existing user projects are not rewritten during the cpuv application update.

--needed keeps an existing compatible lock when the requested project dependencies and constraints are already declared. It verifies and reuses the cached environment, repairs missing files, and works offline when all required content is cached. New dependencies or changed constraints still trigger normal resolution. It is not required to fix package extraction errors.

base-devel is an explicit MSYS2 host-tool request, not a UCRT64 C++ library. cpuv discovers pacman beside the selected MSYS2 compiler, queries whether base-devel is installed, and only if missing runs pacman -S --needed --noconfirm base-devel. This changes that MSYS2 installation, uses pacman's normal package scripts/hooks, and is kept outside the project's dependency lock. Offline mode refuses a missing host-tool installation. MSYS2 host tools are available through cpuv run and activation; unrelated system package managers are not invoked.

MinGW packages such as GLib can include pacman metadata under usr/share/libalpm/hooks or scripts in addition to their compiler-prefix payload. cpuv validates those paths and discards this host metadata for project-library installs; it does not execute those hooks or copy them into the project. Other out-of-prefix payload remains an error.

Commit cpuv.toml and cpuv.lock with your source code. Recreate the environment on another compatible machine with:

~~~powershell
cpuv sync
cpuv sync --offline
~~~

Use cpuv lock to resolve dependencies again intentionally. A lockfile records one package-index snapshot and an ABI fingerprint. It is not a promise that every compiler or future rolling-repository snapshot is binary-compatible. Changing compilers can require regenerating the lockfile and environment. Keep the cache if you need to restore packages that upstream later removes.

## Compile and run controls

~~~powershell
cpuv run g++ solver.cpp -O2 -- first-argument second-argument
cpuv run --no-run g++ solver.cpp
cpuv run g++ solver.cpp -o solver.exe
cpuv run g++ -c solver.cpp
cpuv run g++ solver.o -o solver.exe
~~~

- Arguments after -- are passed to the resulting program. Standard input, output, and the program's exit status are preserved.
- --no-run builds without executing. Compilation failures never execute a previously built program.
- Managed compiler outputs, including an explicit -o output, are placed under .cpuv/build. Object names from an earlier managed compile can be reused.
- Compile-only, preprocessing, assembly, compiler-query, and shared-library commands do not launch a program.
- Other commands can run inside the environment, for example cpuv run cmake --version.

To use the raw compiler in an activated environment:

~~~powershell
. .\.cpuv\activate.ps1
~~~

Raw compiler use follows normal GCC output conventions. Use cpuv run g++ for managed output placement and automatic execution.

## Compiler and ABI compatibility

| Compiler environment | Native compile/run | Dependency installation |
| --- | --- | --- |
| MSYS2 UCRT64, x86-64, POSIX threads, SEH, libstdc++ dual ABI 1 | Supported | Matching UCRT64 binaries, or ConanCenter |
| MSYS2 MINGW64 / standalone MinGW-w64 | Native compiler and runtime retained | ConanCenter with the selected GCC profile |
| Other recognized native Windows GCC distributions or versions | Native compile/run with that compiler; library feature requirements still apply | ConanCenter packages supporting that compiler; no universal ABI guarantee |
| MSVC, Clang, non-Windows hosts, or cross-compilers targeting another OS | Outside version 0.0.2 | Outside version 0.0.2 |

cpuv detects GCC's version, target, architecture, runtime, thread model, exception model, libstdc++ ABI, and supported C++ language level. It records additional compiler ABI macros as evidence. It does not require one GCC version for native compilation. The initial language standard adapts to the detected GCC version and can be changed in cpuv.toml.

Precompiled C++ libraries cannot safely be used across arbitrary ABIs. cpuv chooses a binary provider only for recognized compatible compiler families and checks runtime package versions. When that path does not match, ConanCenter supplies packages for the selected compiler/profile, building missing binaries from upstream source instructions. Absent packages and unsupported configurations fail explicitly. These checks cannot prove every upstream feature configuration compatible. See the [GCC ABI documentation](https://gcc.gnu.org/onlinedocs/libstdc++/manual/abi.html) and [MSYS2 environments](https://www.msys2.org/docs/environments/).

Matching fingerprints are a compatibility filter, not a guarantee. Before committing a managed environment, cpuv inspects PE binaries and COFF archive members for architecture, checks direct runtime imports, and checks named symbol edges between packaged DLLs. The report .cpuv/env/compatibility.json lists evidence and unresolved external DLLs. CPU instruction requirements, all feature/debug macros, packing, ordinal imports, external exports, and LTO compatibility are not proved by these checks. Common ABI-changing compile options are rejected for managed binary environments.

The compiler shim is the primary integration. Generated GCC specs are experimental and only take effect when the real compiler is explicitly invoked with -specs=.cpuv/env/cpuv.specs and CPUV_ENV set. Merely writing a specs file does not change an unmodified g++. Unsupported specs plans fail with a message to use the shim.

GCC remains responsible for selecting and invoking its linker. Driver options such as -fuse-ld are forwarded. The selected linker must support the compiler's target, object format, runtime, and options.

**Having g++ installed is not enough to guarantee every library or linker combination.** cpuv adapts through upstream package metadata and builds missing compatible binaries with the selected GCC. A linker must still work with that GCC, and the compiler must support the library's C++ features. See the [compatibility policy](docs/compatibility.md).

## Community catalog and toolchain checks

~~~powershell
cpuv doctor
cpuv add xtensor
cpuv --catalog add fmt zlib
cpuv run g++ program.cpp
~~~

Missing MSYS2 dependencies select ConanCenter automatically. Use --catalog to select it explicitly for the whole environment. The project then records backend = "conan". Compatible catalog packages are reused; missing binaries build with the selected GCC and its matching gcc. --from-source forces source builds, including upstream build requirements, and can take substantially longer.

cpuv ships no per-library source registry, patches, or handwritten library recipes. ConanCenter maintains upstream build metadata centrally; a catalog entry is still needed for a package. C++ libraries have different build systems and requirements, so a universal build method for arbitrary archives is not possible.

CPPFLAGS, CFLAGS, CXXFLAGS, and LDFLAGS define a saved build profile. Run cpuv sync after changing it. Dependencies and consumers receive that profile; compiler/profile identities separate caches. A version-3 lock embeds upstream revision references, package versions, and portable profile inputs. Local compiler paths remain in .cpuv and the local cache. Older source locks migrate during the next successful synchronization.

cpuv doctor compiles, links, and runs a small program using the actual driver. Use cpuv doctor -- -fuse-ld=bfd to check a driver-supported linker. See [catalog installation details](docs/source-builds.md).

## Configuration

| Setting | Purpose |
| --- | --- |
| --compiler PATH | Select the compiler for toolchain discovery and environment creation |
| CPUV_CXX, then CXX | Compiler selection before searching PATH; specify an executable, without shell flags |
| CPUV_MSYS2_ROOT / MSYS2_ROOT | Help discover an unregistered MSYS2 installation when g++ is absent from PATH |
| -C DIRECTORY | Work in another project directory |
| --cache-dir DIRECTORY | Override the shared cache location |
| --concurrent-downloads N | Limit concurrent downloads; default 50 |
| --max-download-mib N / CPUV_MAX_DOWNLOAD_MIB | Per-archive download limit; default 16384 MiB (16 GiB), streamed to disk |
| --max-extract-mib N / CPUV_MAX_EXTRACT_MIB | Expanded payload limit per archive; default 65536 MiB (64 GiB) |
| --max-extract-files N / CPUV_MAX_EXTRACT_FILES | Maximum entries per archive; default 1000000 |
| --max-index-mib N / CPUV_MAX_INDEX_MIB | Compressed repository metadata limit; default 64 MiB, separate from package sizes |
| --download-timeout-secs N / CPUV_DOWNLOAD_TIMEOUT_SECS | Whole-transfer deadline; default 3600 seconds, 0 disables it |
| --repository URL | Override the package repository base URL |
| --offline | Use cached metadata and archives only |
| --refresh | Fetch fresh index metadata |
| --catalog | Use ConanCenter for the entire environment |
| --from-source / CPUV_FROM_SOURCE | Force source builds through ConanCenter, including build requirements |
| --build-jobs N / CPUV_BUILD_JOBS | Limit parallel compiler jobs in source builds |
| CPPFLAGS, CFLAGS, CXXFLAGS, LDFLAGS | Set the saved source build profile; run sync after changing it |
| --reset-profile | Clear saved flags, then apply the current environment on sync |
| CPUV_SHIM_TRACE=1 | Show compiler invocations for diagnostics |

Run cpuv --help or cpuv COMMAND --help for the complete options. Use cpuv toolchain to inspect the detected compiler and provider. Compiler paths inside generated environment metadata are local to that machine; regenerate the environment after relocating or replacing the compiler.

Without a compiler on PATH, discovery checks MSYS2 environment roots, Windows installation records, and bounded marker-based searches beneath system/profile/program directories. A recognized UCRT64 compiler is preferred for the binary provider; ambiguous candidates require --compiler. No drive, username, or MSYS2 installation directory is embedded in code.

MSYS2 packages and cpuv bootstrap tools share the configurable archive limits. ConanCenter package/source transfers use Conan's own downloader and extraction policy; these cpuv byte/time limits do not wrap upstream recipe downloads. A 2 GiB archive fits within the defaults; larger requirements can raise --max-download-mib and --max-extract-mib. The 64 MiB repository-index limit bounds metadata held in memory and does not cap library archives. Connection and idle-read timeouts remain active even when the overall transfer deadline is disabled. File checksums and indexed sizes remain mandatory.

HTTP is permitted only for loopback endpoints; HTTPS is required elsewhere, including redirects. IPv6 ::1 is an address, not port 1. cpuv is a client and connects to the port in the repository URL; it does not reserve a server port. A local server should bind port 0 so the operating system assigns an available port, then pass its actual URL to --repository. Choosing another server port inside the client would connect to the wrong service.

## Project layout

~~~text
my-project/
  main.cpp
  cpuv.toml
  cpuv.lock
  .cpuv/                 generated, hidden, do not commit
    bin/                 compiler shim
    env/                 dependency headers, libraries, runtime DLLs, metadata
    build/               executables and intermediate artifacts
    toolchain.json       detected local compiler information
~~~

The shared cache lives under the current user's local application data by default. It contains downloaded archives, verified package records, content blobs, and indexes. Do not edit hardlinked package files in place: they may share storage with the cache.

cpuv clean moves loose EXE, DLL, and object files from the project root into a timestamped .cpuv/build/legacy archive. It preserves their content, source files, and project configuration; it does not delete these binaries.

## Development and validation

Rust is a developer dependency only. To build from source, install Git, stable Rust 1.88 or newer, and the Microsoft C++ build tools used by the default Rust MSVC toolchain:

~~~powershell
git clone https://github.com/Bongusrihas/cpuv.git
cd cpuv
cargo build --workspace --release --locked
~~~

Developers can also use cargo install --git https://github.com/Bongusrihas/cpuv.git --locked cpuv-cli. Regular users should use the prebuilt installer above.

~~~powershell
cargo fmt --all -- --check
cargo clippy --workspace --locked -- -D warnings
cargo build --workspace --locked
~~~

scripts/package.ps1 builds the standalone installer and portable ZIP under dist. The native installer never launches a shell; the optional download bootstrap uses Windows PowerShell. Rust is required to build cpuv from source, but not to install or use the prebuilt release.

This source distribution contains all first-party implementation crates, Cargo manifests and lockfile, installer/update code, build/package scripts, and documentation. Third-party Rust libraries are declared in Cargo.toml and pinned in Cargo.lock. No private cpuv component is required to build the application.

The source-only checkout excludes test suites, development simulations, benchmark scripts/results, and generated documents. CI checks formatting, Clippy, and compilation; the release workflow packages the Windows executables. See [CONTRIBUTING.md](CONTRIBUTING.md), [architecture](docs/architecture.md), [overview.md](overview.md), and the [project encyclopedia](docs/project-encyclopedia/MASTER_INDEX.md).

Performance depends on the package graph, cache verification, disk, network, and compiler. No fixed installation latency or performance equivalence with uv is promised.

## Scope and security

Version 0.0.2 uses MSYS2 indexes or ConanCenter metadata. The MSYS2 path verifies archive SHA-256 checksums and validates extraction paths without executing package hooks. Conan executes upstream Python recipes and build code with your permissions, even when it downloads a binary. Its downloader and cache formats are owned by Conan; cpuv checks Conan cache integrity before and after installation and inventories deployed output. Explicit host tooling uses pacman normally. Package signatures, sandboxed/hermetic builds, and universal ABI conversion are not implemented. Use trusted catalogs and upstream sources. Compiler response files are not supported by managed output rewriting.

## Credits and license

**Author:** Bongu srihas

**Co-authors:** Gemini 4 argon and GPT-6 sol

**Version:** 0.0.2

Licensed under [Apache-2.0](LICENSE). Third-party compilers and packages retain their own licenses. See [AUTHORS.md](AUTHORS.md) and [NOTICE](NOTICE).
