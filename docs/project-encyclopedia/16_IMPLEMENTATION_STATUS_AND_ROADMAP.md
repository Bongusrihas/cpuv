# 16 · Implementation status and roadmap

[Index](MASTER_INDEX.md) · [Quality evidence](11_TESTING_AND_QUALITY.md) · [Issue details](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md)

Snapshot: 2026-10-17, application 0.0.2 working tree, base `4889f1f`. The version is not evidence that a corresponding GitHub release was published. Status labels describe the inspected source and checks, not every machine.

## Implemented and verified

| Feature | Evidence and dependency | Missing work / next step |
| --- | --- | --- |
| `rm` alias of direct-dependency removal | `Action::Remove` and executed MSYS2/Conan removal scenarios | Keep aliases compatible; add reproducible regressions if distribution policy permits |
| Removal graph pruning and shared dependencies | Controlled MSYS2 roots sharing common; offline catalog graph change | Broaden real-package/component cases |
| Validation preserves state before preparation | Invalid transitive/mixed removals left manifest/lock/metadata hashes unchanged | Fault-inject failures during swap, not just input validation |
| Last-root removal | Both providers committed empty dependency environments | Verify additional explicit link/profile combinations |
| Remaining dependency compiles after removal | Real cached catalog JSON dependency and GCC | Expand compiler/runtime matrix |
| Static checks and release compilation | rustfmt, Clippy warnings denied, locked release workspace build | Verify minimum Rust and fresh OS prerequisites |
| Updated local command and packaging | Installed `rm --help`; package script completed | Publication remains a separate maintainer action |

## Implemented but insufficiently tested by this audit

The compiler probe/doctor, complete managed runner flag surface, MSYS2 network retry paths, large archive limits, safe link handling, full provider component mappings, raw-shim DLL deployment, native registry PATH operations, and self replacement have concrete implementations. Earlier session checks exercised several of them, but this audit did not rerun a full matrix. Their dependencies include Windows APIs, actual GCC distributions, upstream metadata, and executable publishers. Highest priority is a reproducible behavioral suite rather than additional optimistic claims.

## Partial or bounded implementations

| Area | State/evidence | Suggested next step |
| --- | --- | --- |
| ABI compatibility evidence | Fingerprints, runtime checks and PE/COFF analysis exist; no universal proof | Keep explicit limits; add profile-specific executable validation |
| Catalog static archive inspection | PARTIALLY IMPLEMENTED: nested catalog `.a/.lib` misses root-lib branch | P1: traverse owned archive files regardless of deployment nesting |
| Transactions | Normal-error swaps work; binary/catalog implementations and recovery guarantees differ | P1: common transaction abstraction with retained backups/fault cases |
| Offline mode | Cached first-party/provider operation works; external recipes are not network-isolated | P2: decide whether stronger offline isolation is a product requirement |
| Static/private linking and package options | Public interfaces implemented; no arbitrary CMake evaluation or general Conan option interface | P2: explicit validated option/link model before widening support |
| Environment reuse validation | CAS hashes verified; local generated metadata mostly existence-checked | P1: verify metadata inventory or regenerate it |
| Configuration consistency | `--refresh` is MSYS2-focused; detailed linkage label does not force provider artifacts | P2: align/document behavior and reject misleading declarations |

## Absent, deferred, or unknown

- **NOT FOUND:** first-party web/API server, database service, daemon, account system, self-uninstall command, retained test suite, coverage report, automatic cache GC, log rotation, Authenticode/signature verification, sandbox/hermetic build execution.
- **Explicit current scope exclusions:** MSVC/Clang adapters, vcpkg, arbitrary archive inference, Cygwin catalog builds, cross-OS compilation, universal GCC/linker/ABI compatibility. A boundary exclusion is not necessarily a committed future feature.
- **NOT FOUND as stubs:** no `todo!`/`unimplemented!` production placeholders surfaced in the source scan. Deliberate rejection branches are limitations, not mocked working features.
- **UNVERIFIED:** all upstream library recipes, live release infrastructure, minimum OS/Rust matrix, arbitrary GCC/linker versions, third-party vulnerability status, concurrent mutation under every Windows lock condition.

## Recommended roadmap, not historical commitments

1. P1 correctness/recovery: fix nested archive inspection, metadata reuse validation, and backup retention; maintain reproducible filesystem failure and removal regressions.
2. P1 assurance: make the behavioral validation strategy available to contributors while respecting the source distribution's test policy. Current CI cannot establish functional coverage.
3. P2 interface consistency: define catalog option/linkage controls, refresh semantics, package-name transitions, and stronger lock/provenance guarantees before new provider support.
4. P2 operational safety: shared environment-use locking, explicit cache retention/repair controls, signed release provenance and log redaction/rotation requirements.
5. P3 measured optimization: establish controlled cold/warm costs and compiler-wrapper overhead before changing concurrency or integrity work.

All new roadmap priorities are audit recommendations. No extra application fixes were made as part of writing this encyclopedia. The sole new application change for this request is the authorized `rm` alias; prior dirty source changes belong to the earlier catalog implementation.
