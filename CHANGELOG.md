# Changelog

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
