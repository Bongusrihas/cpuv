# Compiler, linker, and ABI compatibility

Version **0.0.2** does not support every possible Windows/GCC/linker/ABI combination. Installing g++ alone does not guarantee that a precompiled dependency is compatible.

## Application platform

The prebuilt cpuv application targets Windows 10 or newer on x86-64. It does not require Rust, Cargo, Git, or a separate Visual C++ runtime installation. Building cpuv from source requires Rust and its platform build tools. Compiling C++ programs requires a separate working GCC toolchain; cpuv does not install or bundle GCC.

## Supported combinations

| Component | Version 0.0.2 boundary |
| --- | --- |
| Compiler discovery | Explicit executable, CPUV_CXX/CXX, PATH, then discovered MSYS2 installation roots and Windows registration |
| Native compilation | Recognized GCC distributions use their own compiler/runtime; language defaults adapt to observed capabilities |
| Managed prebuilt dependencies | MSYS2 UCRT64, x86-64, POSIX threads, SEH exceptions, libstdc++ dual ABI 1, with matching runtime package versions |
| Source dependencies | ConanCenter packages built/reused with the selected native MinGW GCC and saved profile; package/compiler support comes from upstream metadata |
| Linker | Selected and invoked by GCC; must support the target, object format, runtime, and driver options |
| Other C++ ecosystems | MSVC, Clang, other managed binary providers, and cross-OS compilation are outside this release |

When a compiler has no supported binary provider, the repository runtime does not match, or a custom profile is requested, cpuv selects ConanCenter. Every dependency must exist there and support that compiler's features. An older GCC cannot build a library requiring an unsupported language mode. No incompatible UCRT64 binary is silently substituted for an absent or unsupported catalog package.

## Why any ABI is not possible

C++ ABI includes object layout, symbol naming, exceptions, standard-library configuration, and other compiler/build choices. Matching a compiler name or fingerprint does not prove that every such setting matches. MSYS2 also distinguishes UCRT and MSVCRT environments; their object/static-library payloads cannot be freely mixed. See [GCC ABI policy](https://gcc.gnu.org/onlinedocs/libstdc++/manual/abi.html) and [MSYS2 environments](https://www.msys2.org/docs/environments/).

cpuv filters binary candidates, checks selected runtime versions, inspects PE/COFF architectures and packaged symbol edges, and rejects common incompatible build options. Source mode builds dependencies and consumers with the saved profile instead of reusing a foreign ABI. It does not convert existing binaries or prove all upstream target-specific options, external DLLs, CPU instruction requirements, or library features compatible.

The source profile and compiler observations form a compiled-cache identity. Changing the compiler or saved flags rebuilds dependencies. An unset profile variable retains its locked value; sync --reset-profile clears the saved profile before applying current environment values. Common conflicting consumer ABI options are rejected. This is a bounded guard, not complete recognition of every GCC option.

## How the linker is used

cpuv supplies the planned include directories, library directories, definitions, and linker flags to the recorded g++. GCC then invokes its own linker. A driver-supported alternative can be selected with options such as -fuse-ld when that toolchain provides a compatible implementation. An arbitrary linker executable, including Microsoft link.exe, is not interchangeable with a GCC toolchain merely because it is on PATH. See [GCC link options](https://gcc.gnu.org/onlinedocs/gcc/Link-Options.html).

## After installing cpuv

1. Run cpuv init to discover and record the real compiler.
2. Run cpuv toolchain to inspect its target, runtime, ABI observations, provider, and linker.
3. Run cpuv doctor to compile, link, and execute a small native program. Additional flags after -- check the driver's actual linker setup.
4. Run cpuv add for compatible MSYS2 binaries or ConanCenter packages. Use --from-source explicitly when desired.
5. Run cpuv run g++ program.cpp to compile and execute with the project's package metadata and DLL paths.

Catalog mode bootstraps standalone Conan, CMake, and Ninja into the cache. It requires no system Python or Rust. ConanCenter maintains package build instructions; cpuv contains no per-library recipes. Some upstream builds need additional system tools. Cygwin and cross-OS catalog builds are not supported by this adapter. See [catalog installation](source-builds.md).

Open-source licensing gives users the right to inspect, modify, build, and redistribute cpuv under Apache-2.0. It does not imply universal binary compatibility. The repository contains all first-party implementation code; third-party dependencies retain their own licenses.
