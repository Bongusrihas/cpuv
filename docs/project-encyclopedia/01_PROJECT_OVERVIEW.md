# 01 · Project overview

[Index](MASTER_INDEX.md) · [Evidence and coverage](COVERAGE_MATRIX.md)

Verified 2026-10-17 against the 0.0.2 working tree on `main`, based on commit `4889f1f`. The catalog backend and `rm` alias are uncommitted changes, not features established by that commit alone.

## Purpose and users

cpuv is a local Windows command-line application for people using native GNU C++ compilers. It manages dependency environments and supplies package interfaces to compilation. Its managed runner keeps executables and intermediate files under a hidden `.cpuv` directory and runs successful program links immediately. It is useful for small C++ programs, projects sharing downloaded dependencies, and contributors inspecting compiler/runtime compatibility.

The problem is partly package installation and partly integration: downloading an archive does not tell GCC which headers, definitions, libraries, and runtime DLL directories an application needs. cpuv joins those steps using repository metadata, a recorded compiler, a generated link plan, and a managed command runner. This purpose is documented in the existing [README](../../README.md) and implemented in CLI `sync` and runner `run`.

## Capabilities and evidence

| Capability | Status | Evidence |
| --- | --- | --- |
| Project initialization, compiler observation, activation | IMPLEMENTED | `main.rs::init`, `cpuv-toolchain::probe_with_flags` |
| MSYS2 snapshot dependencies and bounded parallel downloads | IMPLEMENTED | `cpuv-solver::resolve_for`, `Fetcher::download_all` |
| Automatic ConanCenter fallback and source builds | IMPLEMENTED, bounded catalog coverage | `source::sync`, `configure_profile`, `prepare_metadata` |
| Add, remove, and the `rm` alias | IMPLEMENTED; removal executed on both backends | `Action::Remove`, `execute`, `sync`; [verification](11_TESTING_AND_QUALITY.md) |
| Content-addressed reuse and offline operation | IMPLEMENTED with provider differences | `Store::cached`, `Store::materialize`; Conan `--no-remote` |
| Managed compile/run and clean output placement | IMPLEMENTED | `runner::plan`, `compiler_args`, `runner::run` |
| Native setup and verified self update | IMPLEMENTED; not fully retested in this audit | `install::install`, `update::run` |
| Universal library, ABI, or linker support | NOT FOUND | Explicit compatibility restrictions; upstream catalog requirements |

## System shape

Seven Rust crates compile into a principal `cpuv` executable plus an optional standalone shim executable. Most library calls stay inside one process. External processes are GCC, generated programs, standalone Conan, upstream build tools, and explicitly requested MSYS2 pacman. There is no first-party web server, application database, daemon, scheduler, or account service.

MSYS2 is the first provider for recognized UCRT64 x64 POSIX/SEH compiler profiles. If a requested root is absent, runtime eligibility differs, custom flags are supplied, or `--catalog` is chosen, the whole dependency graph uses ConanCenter. cpuv contains generic adapter code and pinned bootstrap tools; it no longer contains per-library source recipes. ConanCenter still supplies upstream package-specific build instructions. The two package graphs are not combined.

## Requirements and boundaries

The distributed executable targets x64 Windows, with Windows 10 or newer documented as the application requirement. Users need a complete working native GCC installation for compilation. The prebuilt cpuv installation needs no Rust, Cargo, Git, system Python, or separately installed Microsoft C++ runtime. Building cpuv itself requires Rust 1.88 or newer and the Windows MSVC build prerequisites. Compiler language requirements still come from each library.

MSVC/Clang integration, arbitrary GitHub archive inference, vcpkg integration, cross-OS builds, and Cygwin catalog builds are outside this implementation. A GCC driver must invoke a compatible linker. An ABI fingerprint is an observation-based filter, not proof that every upstream build option matches.

## Risks and current state

The working tree is substantial functional code rather than a prototype made of stubs, but its source-only checkout has no retained automated test suite. CI checks formatting, Clippy, and compilation. Checksums do not authenticate compromised repositories. Conan recipes execute upstream code with user permissions; builds are not sandboxed. Catalog static archives also fall outside one current binary-inspection path. See [security](09_SECURITY_AND_TRUST_MODEL.md), [status](16_IMPLEMENTATION_STATUS_AND_ROADMAP.md), and [known issues](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md).

Primary sources: [CLI](../../crates/cpuv-cli/src/main.rs), [catalog adapter](../../crates/cpuv-cli/src/source.rs), [runner](../../crates/cpuv-cli/src/runner.rs), [compatibility policy](../compatibility.md).
