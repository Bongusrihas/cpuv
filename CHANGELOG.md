# Changelog

## 0.0.4 — 2026-10-24

- Correct ordered/static metadata linking, scoped flags, pkg-config expansion and paths containing spaces.
- Preserve compiler stdout, response-file arguments and shim runtime deployment; reject colliding outputs before compilation.
- Add journal recovery, process-lifetime reader leases, complete environment/request/tool/SDK verification and safe cache quarantine/repair.
- Enforce frozen sync and explicit lock updates; verify Conan shared/static artifacts and custom Cargo target installation.
- Add profiles, MSVC/GNU-style Clang adapters, CMake/CTest, pinned vcpkg integration, plan/verify and application runtime bundles with documented limits.
- Retain unit/native/provider/installer acceptance gates and CI release evidence. Keep library-specific recipes and fixes upstream.
- See [0.0.4 release notes](docs/release-notes/v0.0.4.md) for validation and remaining compatibility boundaries.

## 0.0.3 — 2026-10-19

- Classify archive candidates by their file signature, preserving text licenses with .LIB extensions while retaining malformed-library and architecture checks.
- Separate Conan target and native build-tool profiles, including 32-bit GCC targets on x64 Windows; allow --build-compiler/CPUV_BUILD_CXX to select the native build compiler.
- Declare checksum-verified bootstrap tools through Conan platform tool requirements using their verified versions, avoiding unnecessary or unsupported provider tool binaries.
- Add MSYS2 MINGW64 binary selection for observed compatible MSVCRT GCC profiles, separate from UCRT64.
- Add toolchain list/install/use/reset commands, pacman-capable host discovery, dry-run/offline installation, and verified compiler selection without PATH edits.
- Expose standalone Conan commands using the same managed compiler/profile home as add/sync, including remotes and recipe export/create.
- Accept full Conan recipe references and manifest reference declarations without package-specific build rules.
- Enforce opposing CRT import policies and inspect nested library archives in provider deployments.
- Keep package metadata/build recipes upstream; exclude Nix/WSL from this Windows release.

## 0.0.2 — 2026-10-18

- Refresh cached negative MSYS2 package lookups once online before automatic catalog fallback; retain offline behavior and preserve the cache on refresh failure.
- Report the exact missing requirements and repository suggestions when catalog fallback fails, preserving both providers' diagnostics and distinguishing catalog resolution from compilation failures.

- Add cpuv uninstall and a read-only --dry-run preview, Windows self deletion, recognized companion cleanup, and user PATH cleanup that preserves shared tool directories and project/cache data.
- Add cpuv rm as an alias for dependency removal, retaining shared dependencies and caches while rebuilding the remaining environment.
- Remove cpuv-owned library recipes, library patches, custom recipe registries, and --recipes.
- Fall back automatically from missing MSYS2 packages to ConanCenter for the whole environment; --catalog selects it explicitly.
- Bootstrap standalone Conan alongside CMake/Ninja without requiring system Python or Rust.
- Resolve transitive requirements and public interfaces from upstream metadata, build missing compatible binaries, and deploy transactionally.
- Record catalog revision locks and saved compiler profiles, check provider cache manifests, and preserve offline binary-cache reuse.
- Prevent one-letter substring matches from producing unrelated missing-package suggestions.
- Add a project encyclopedia covering source structure, architecture, interfaces, operations, verification, and documented limitations, with a separate application uninstall update.

## 0.0.1

Initial public source release.

- Portable Windows GCC discovery and compiler ABI fingerprints.
- Managed binary packages scoped to MSYS2 UCRT64 with compatibility filters and automatic recipe-backed source fallback for other native GCC profiles.
- Source recipes for fmt, Eigen, and zlib, custom registries, pinned CMake/Ninja bootstrap, saved ABI flags, verified compiled caches, and portable source locks.
- A doctor command performs real compiler/linker/runtime checks; language defaults adapt to compiler capabilities.
- Bounded parallel downloads, integrity verification, offline reuse, and content-addressed storage.
- Managed compile-and-run commands with hidden build outputs and runtime DLL lookup.
- Exact package resolution and repository-based suggestions; no per-library aliases, automatic sibling packages, special include paths, or named linker defaults.
- MSYS2 discovery through environment roots, Windows registration, and installation markers without a fixed compiler path.
- Configurable multi-gigabyte download/extraction limits, separate metadata limits, shared loopback address validation, and longer transfer deadlines.
- Source and Cargo Git installation, a single-executable compiler shim, and developer build scripts.
- Native setup executable, one-command prebuilt download, portable ZIP, static C runtime, and automated release packaging; no Rust installation required for users.
- GLib/Qt pacman hook metadata handling while preserving prefix and traversal checks.
- --needed add mode and explicit base-devel delegation to the selected MSYS2 host pacman.
- Verified GitHub release self updates with Windows executable replacement and failure-preservation checks.
- Authoritative package metadata link plans, explicit module selection, and PE/COFF inspection.
- Experimental specs require explicit loading; compatibility boundaries are documented without universal ABI or fixed-latency claims.
- Source distribution includes the complete runtime implementation and excludes test suites, recorded benchmarks, and generated reports.
