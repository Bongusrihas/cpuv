# Building dependencies with the installed GCC

cpuv can build recipe-backed libraries using the selected GNU g++, its native runtime/ABI, and its driver-selected linker. This supports compiler distributions that cannot use MSYS2 UCRT64 binaries. It does not convert existing binaries or make unsupported C++ features available in an older compiler.

## Use source builds

~~~powershell
cpuv init --from-source
cpuv add fmt eigen zlib
cpuv run g++ program.cpp
~~~

For an existing project, cpuv add --from-source fmt switches the dependency backend after a successful source installation. Every existing dependency must have a source recipe. A failed resolution or build preserves the project configuration and active dependency environment.

Source mode is automatic when the detected GCC has no supported prebuilt provider, when an existing project changes compilers, when a custom build profile is requested, or when repository runtime versions do not match the compiler. The project records backend = "source". Source mode remains selected for subsequent operations.

The embedded starter registry provides **fmt 11.1.4**, **Eigen 3.4.0**, and **zlib 1.3.1**. Additional libraries require recipes. No source recipe is guessed from the spelling of a package or header. The default fmt and zlib recipes build shared libraries; Eigen installs headers. fmt requires a compiler capable of C++11. zlib also needs the matching gcc C compiler and its normal Windows build utilities.

## Compiler and ABI profile

Select a compiler with --compiler, CPUV_CXX, CXX, or PATH. cpuv always invokes that recorded executable and lets it choose its linker. A compiler change on source sync rebuilds the dependency environment for the newly selected toolchain.

Native/source projects start with project.std = "default", preserving the compiler's language default. If a recipe requires a newer supported language mode, cpuv selects that minimum mode for both dependencies and consumers. An explicitly configured project standard or -std= in CXXFLAGS takes precedence and must satisfy the recipe. A library that needs features absent from the compiler cannot be built without choosing another library version or compiler.

CPPFLAGS, CFLAGS, CXXFLAGS, and LDFLAGS define the source profile. CPPFLAGS applies to both C and C++; CFLAGS applies to C, CXXFLAGS to C++, and LDFLAGS to link steps. Public C/C++ ABI macros must be applied consistently across relevant languages. Windows command-line quoting is used when parsing these variables.

~~~powershell
$env:CXXFLAGS = '-O2 -D_GLIBCXX_USE_CXX11_ABI=0'
cpuv sync
cpuv run g++ program.cpp
~~~

The lock preserves the profile, so an unset variable in a later terminal retains the saved setting. Where the shell supports empty environment values, an empty value clears that part of the profile. To reliably reset all saved flags, remove the profile variables from the current environment and run cpuv sync --reset-profile. Run sync after changing settings to rebuild. The resulting compiler flags are also applied when compiling the application. Common incompatible ABI-changing command-line overrides are rejected rather than silently linking against the old profile.

cpuv does not add CMake's implicit Release optimization/NDEBUG flags to the profile; choose such flags explicitly if wanted. Upstream target-specific options and feature choices remain the recipe author's responsibility. Source builds provide matching build inputs, not a universal ABI proof.

## Build tools, downloads, and cache

Pinned CMake and Ninja executables are downloaded automatically into cpuv's cache and verified by SHA-256. Users do not need Rust, Git, Python, a global CMake installation, or a global Ninja installation for these starter recipes. The automatic tool bootstrap currently targets x64 Windows. A complete working GCC toolchain and any extra tools required by a particular recipe are still necessary.

Source archives download concurrently using the configured --concurrent-downloads limit. Compilation uses --build-jobs or CPUV_BUILD_JOBS, defaulting to available CPU parallelism. Archives use bounded extraction and path validation; links and special payload entries are rejected. Upstream license files are retained in share/licenses inside the dependency environment.

Source and tool archives share --max-download-mib (16384 MiB by default), --max-extract-mib (65536 MiB), and --max-extract-files (1000000). Downloads stream to disk, including archives larger than 2 GiB. --download-timeout-secs defaults to 3600; 0 disables the whole-transfer deadline while retaining connection/read timeouts. Repository-index limits apply only to binary registry metadata, not source or tool archives. Custom recipes remain the supported place for library-specific build options, source patches, and public interfaces; the generic resolver and linker contain no per-library branches.

Compiled environments are keyed by the compiler executable, observed toolchain configuration, source recipes, generator revision, and build profile. Cached output contents are verified before reuse. cpuv sync --offline can reuse a complete compiled cache or build from already cached archives/tools. It fails if required inputs are missing; it does not contact the network as a fallback.

Source-build logs stay under the shared cache's source/logs directory. Dependency artifacts live in .cpuv/env, and application executables stay in .cpuv/build. These directories are machine-local and excluded from the source repository.

## Add or override a recipe

The complete default registry is [recipes.toml](../recipes.toml), embedded into the executable. A custom TOML file with schema = 1 can supply packages and optional tool overrides; it extends the embedded registry.

Copy a package block from recipes.toml and specify the actual version, HTTPS archive URL, SHA-256, archive type, strip_components, license_file, dependencies, minimum_cxx, and CMake options for the library. Set requires_c when the project uses C. Recipe options cannot replace the selected compiler, install prefix, or ABI profile.

Optional patches contain a relative file, an exact find string, and a replacement string. Each match must occur exactly once in the checksum-pinned source; a changed upstream file fails before configuration. Patches are included in locks and cache identities. The zlib recipe uses this mechanism to pass relative resource paths to windres, avoiding distribution-specific quoting failures beneath directories with spaces.

~~~powershell
cpuv --from-source --recipes my-recipes.toml lock
cpuv sync
~~~

A recipe can provide an interface table with a pkg-config module name, public libraries, Cflags, Requires modules, and optional libraries_by_runtime overrides. This replaces that package's own module of the same name, wherever upstream installed it. It cannot overwrite another package's interface. Libraries without an explicit interface use upstream installed pkg-config metadata. Ambiguous exports require explicit project [link] selection.

The portable cpuv.lock records exact recipes, archive checksums, the profile, and compiler observations. Machine-specific compiler/cache paths are stored only under .cpuv. Locked source recipes allow sync without the original custom registry file. Run lock with the registry again to update the pinned recipes.

Source builds execute upstream CMake/build code. SHA-256 pins content; it does not certify that upstream code is safe. Use trusted recipes and upstream projects. Source builds are not sandboxed, hermetic builds, or universal support for every package, linker, or Windows version.
