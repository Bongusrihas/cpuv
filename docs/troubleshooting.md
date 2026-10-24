# Troubleshooting cpuv

This guide describes the current 0.0.4 implementation. A source push does not update downloaded executables or existing GitHub release assets.

## Check the executable and compiler first

In PowerShell:

```powershell
Get-Command cpuv -All | Select-Object CommandType, Source
where.exe cpuv
cpuv --version
cpuv toolchain
cpuv doctor
```

The first two commands reveal competing installations or shims. Self update replaces the invoked executable; another copy can still be selected by a different terminal. Upgrade older releases to 0.0.4 with:

```powershell
cpuv self update
```

For a locally built update, run the rebuilt setup executable or use the packaged portable executable directly. A Git checkout alone does not change the installed application. See [installation and publishing](releases.md).

## A text license was treated as a library

An older build could report an error such as:

```text
Unsupported library archive .../share/licenses/.../COPYING.LIB: Invalid magic number
```

The old inspector selected files by extension and tried to parse a text license as a binary archive. The maintenance implementation checks archive signatures first. Non-archive documents outside library directories remain installed and appear in .cpuv/env/compatibility.json under non_archive_files_skipped. Genuine nested archives still undergo COFF architecture checks; malformed files in library directories remain errors.

Update cpuv, then rerun the failed add command or cpuv sync for an already declared dependency, using the same target compiler. Removing a license or clearing the dependency cache is unnecessary for this classification error. There are no package-name or license-filename exceptions in the fix.

## Conan requests an unsupported 32-bit CMake package

The upstream message can say:

```text
CMake binaries are only provided for x86_64 and armv8 architectures.
Consider using the system cmake with [platform_tool_requires] section in your profile.
```

A 32-bit GCC produces target libraries and programs, but build tools run on the build machine. Older cpuv code used the target profile for both contexts, causing Conan to request x86 CMake for an x86 target on x64 Windows.

Current cpuv writes separate host and build profiles. On x64 Windows, the host can remain x86 while the build profile uses x86_64. The verified bootstrap CMake/Ninja versions are declared through Conan's platform tool requirements in both profiles. Their actual versions must satisfy the upstream recipe constraints; other tool versions resolve through the native build profile.

Inspect the profiles with:

```powershell
cpuv conan profile show
```

Use the same --compiler selection used for dependency installation. If another native GCC is needed to compile additional build tools, select it independently. These PowerShell commands prompt for your actual compiler paths:

```powershell
$targetCxx = Read-Host 'Full path to target g++.exe'
$nativeCxx = Read-Host 'Full path to native x64 g++.exe'
cpuv --compiler $targetCxx --build-compiler $nativeCxx conan profile show
cpuv --compiler $targetCxx --build-compiler $nativeCxx sync
```

CPUV_BUILD_CXX is the environment-variable equivalent of --build-compiler. Target flags are not applied to native build-tool compilation. A custom cpuv conan -pr:b profile remains authoritative and must describe the tools you intend to run. See [catalog profiles](source-builds.md#compiler-and-profile).

## Missing compatible binaries followed by Build

Conan messages such as No compatible configuration found, a missing binary package, or Build describe a source-build plan when cpuv uses --build=missing. They do not by themselves mean the package name was wrong or the operation failed. An Invalid build requirement or a later ERROR line identifies a separate failure.

Read the provider error and the complete build log at the path cpuv prints. Package version availability, source download failures, compiler features, and required tools can each prevent an upstream build. Switching a 32-bit GCC to unrelated x64 library binaries would not fix those requirements.

Maintenance verification covered a real OpenBLAS install and consumer, a real 32-bit GCC 16.2 CMake/Ninja build, and ConanCenter graph resolution with GCC 6/x86/gnu14 settings. The graph check did not compile with GCC 6.3. Correct tool selection does not guarantee that every library supports that older compiler. See [compatibility](compatibility.md) and [release verification](release-notes/v0.0.3.md).

## A package name or version is missing

Use the exact name published by the selected provider. cpuv refreshes a cached negative MSYS2 lookup once online before falling back to Conan for the entire environment. Repository suggestions are names to investigate, not automatic aliases. Names and versions can differ between providers.

An error saying a Conan requirement could not be resolved happens before compilation. Check the catalog, version constraint, configured remotes, network access, and the log. It does not establish an ABI or linker incompatibility, and cpuv cannot infer build instructions for a package absent from the selected catalog. See [provider selection](source-builds.md#provider-selection).

CI runs a missing-recipe regression against a synthetic package name, verifies unchanged project/lock/environment receipts, and rechecks a native consumer after rejection. This protects failure handling; it does not add missing recipes to an upstream catalog. To use an unpublished C++ library, provide an upstream/local Conan recipe through `cpuv conan export` and add its full reference, or build/install through the library's documented process. Python wheels and R packages are not interchangeable C++ catalog entries.

## Find the relevant diagnostics

| Diagnostic | What it explains |
| --- | --- |
| cpuv toolchain / cpuv doctor | Selected GCC observations and an actual compile/link/run probe |
| cpuv conan profile show | Target and native build profiles in the managed provider home |
| Printed catalog build-log path | Complete provider output, including the failing requirement or build step |
| .cpuv/env/compatibility.json | Prepared environment's binary inspection and skipped non-archive documents |
| .cpuv/env/link-plan.json | Public modules and libraries selected from package metadata |

Environment reports describe the active prepared environment; a failed staged installation does not produce a new successful report. Review paths, flags, and any provider credentials before sharing logs.

## A frozen request or environment verification fails

Use `cpuv lock` after intentionally changing dependency declarations, linkage/options, compiler flags or configuration. `sync` restores the selected lock and rejects disagreement; `--refresh` does not update that graph. Check `cpuv plan` and `cpuv verify` for installed state.

If generated metadata, payloads or DLLs were modified, run `cpuv sync` to repair from verified cached/downloaded content. Offline repair requires all needed cache inputs. Damaged MSYS2 package/blob content is retained under the shared cache's `store/quarantine`; missing offline content fails explicitly. Keep journals/backups when recovery fails, release conflicting processes and retry; do not delete recovery data to silence the error.
