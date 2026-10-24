# Windows providers and toolchains

Version 0.0.4 supports native GNU C++ workflows with MSYS2 UCRT64, MSYS2 MINGW64, and standalone Conan. Compiler observations select the binary repository. MSYS2 dependencies and Conan recipes remain upstream metadata; cpuv has no per-library aliases, patches, or recipe table. Nix/WSL and Linux packages are outside this Windows release.

## Discover and install GCC

```powershell
cpuv toolchain
cpuv toolchain list
cpuv toolchain install --environment ucrt64 --dry-run
cpuv toolchain install --environment ucrt64
cpuv toolchain install --environment mingw64
```

Installation requires an existing MSYS2 host with pacman; GCC need not already be installed. Roots are discovered from environment variables, PATH, registration, and MSYS2 marker files. Multiple roots require --root DIRECTORY. Install MSYS2 through its official installer if no host exists. No installation drive or username is assumed.

The default request is the environment's GCC package and its upstream dependencies, including compiler/runtime development files and binutils. Repeat --package NAME to request other exact upstream packages. cpuv invokes pacman -S --needed --noconfirm without a partial -Sy upgrade, verifies the resulting compiler, and runs a compile/link/execute probe. Host transactions are separate from project transactions. An outdated MSYS2 host may require its normal full system upgrade first. Offline installation requires the requested packages to be already installed.

Use --use during installation to select the compiler, or select an existing one:

```powershell
cpuv toolchain use 'D:\tools\gcc\bin\g++.exe'
cpuv toolchain reset
```

Replace the example path with your compiler. Selection records the verified executable in per-user storage without editing Windows PATH. Precedence is --compiler, CPUV_CXX, CXX, cpuv selection, PATH, and discovered MSYS2 compilers. Project pins/environments are not rewritten by selection; use the original compiler or explicitly resolve a new profile with cpuv lock. Changing the default affects subsequent commands in other projects. Compiler and cache state are retained when uninstalling the application.

## Binary providers

| Provider | Required observations | Target |
| --- | --- | --- |
| UCRT64 | x86-64 GCC, POSIX, SEH, UCRT, libstdc++ dual ABI 1 | Windows |
| MINGW64 | x86-64 GCC, POSIX, SEH, MSVCRT, libstdc++ dual ABI 1 | Windows |
| Conan | Native MinGW GCC and a recipe supporting its profile | Windows |

UCRT64 and MINGW64 have separate namespaces and snapshots. Direct imports of an opposing C runtime or incompatible Microsoft C++ runtime are rejected. Nested COFF archives, including Conan deployments, are inspected. This is compatibility evidence rather than proof for arbitrary ABI settings or external DLLs.

Missing MSYS2 root packages, custom flags, another supported GCC profile, --catalog, or an explicit Conan reference select Conan for the entire graph. MSYS2 and Conan graphs are never combined. Installing a toolchain does not create a package missing from upstream catalogs.

## Conan packages, remotes, and recipes

Initialize a project before configuring its provider so commands share the saved language/profile inputs:

```powershell
cpuv init
cpuv conan --version
cpuv conan remote list
cpuv --catalog add fmt
cpuv run g++ program.cpp
```

Standalone Conan embeds Python; CMake/Ninja are bootstrapped too. Prebuilt cpuv users need no system Rust or Python. cpuv conan delegates arguments in the managed compiler/profile home. The default host profile records the selected target GCC and flags; a separate build profile selects native build tools. A 32-bit target does not request 32-bit CMake on x64 Windows. --build-compiler PATH or CPUV_BUILD_CXX selects the native GCC when needed. Explicit upstream build-profile arguments override the generated build profile. Export/create/add/remote commands must use the same project and compiler/profile. A changed target profile can select a different home needing its own remote/recipe setup. The command prints that home and both profiles.

Verified bootstrap tool versions are declared as Conan platform tool requirements. Upstream requirements still decide whether a supplied version is acceptable. Missing package binaries followed by Build are normal source-build planning; an Invalid build requirement is a separate error and must be addressed before compilation. This separation does not make an old GCC support language features it lacks.

Use an actual trusted registry URL in place of this placeholder:

```powershell
cpuv conan remote add myregistry https://packages.example.org/conan
cpuv conan remote login myregistry YOUR_USERNAME
```

Conan owns remote configuration, authentication, and recipe execution. Keep credentials in provider storage rather than source control. Remote management does not publish packages; publication requires an explicit upstream command.

Use a trusted existing upstream or local recipe, substituting its actual reference:

```powershell
cpuv conan export .\vendor\library --user myteam --channel stable
cpuv conan create .\vendor\library --user myteam --channel stable --build=missing
cpuv add 'library/1.2.3@myteam/stable'
```

A full recipe reference can include #REVISION. It selects Conan automatically and is saved under the package name. Or declare it directly and run cpuv lock:

```toml
[dependencies]
library = { reference = "library/1.2.3@myteam/stable" }
```

Omit a separate version constraint for an explicit reference. Its package name must match the dependency key. The lock records upstream revisions; local recipe exports/binaries are provider state and must be distributed separately to other machines. cpuv.lock does not bundle a private recipe.

Native search/list/profile/cache and other Conan commands are available; a bare command shows upstream help. With cpuv --offline, install/create/build receive --no-remote; export, profile, cache, help/version, and remote list are permitted locally. For other operations use upstream offline flags explicitly. Offline mode cannot sandbox recipe code or subprocesses.

Recipes run with your permissions. Unknown packages still need a catalog entry or an explicit recipe; cpuv cannot infer build instructions from a name. See [compatibility](compatibility.md), [catalog details](source-builds.md), [troubleshooting](troubleshooting.md), and [security](../SECURITY.md).

Upstream references: [MSYS2 environments](https://www.msys2.org/docs/environments/), [Conan remotes](https://docs.conan.io/2/reference/commands/remote.html), and [Conan export](https://docs.conan.io/2/reference/commands/export.html).

## Additional 0.0.4 lanes

MSVC and GNU-style Clang use their own compiler/runtime observations through Conan. `--profile NAME` selects an isolated named profile; `--configuration Release|Debug` selects dependency configuration. Existing projects retain their saved compiler unless explicitly changed. `cpuv lock` intentionally resolves a changed request; `cpuv sync` restores the selected compatible lock.

`--provider vcpkg` selects a separate whole-graph MSVC provider with a pinned configurable registry revision, native versions/features and CMake integration. Git and Microsoft C++ Build Tools are prerequisites. Use provider imported targets through `cpuv build`/`cpuv test`; offline rebuilding of a missing vcpkg tree is not implemented. See [compatibility](compatibility.md) and [README examples](../README.md).
