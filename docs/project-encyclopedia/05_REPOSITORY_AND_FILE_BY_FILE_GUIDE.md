# 05 · Repository and file guide

[Index](MASTER_INDEX.md) · [Full file coverage](COVERAGE_MATRIX.md) · [Symbol lookup](21_REFERENCE_CATALOG.md)

## Root and tooling

The inspected baseline contains 52 present source/configuration/documentation files outside ignored build output. `.git` is repository metadata, not application state. `dist` is ignored release output; Cargo build output may be redirected through `CARGO_TARGET_DIR`. No vendored Rust dependency tree, generated source directory, retained tests, or production database migrations were found.

| File/group | Role, consumers, and modification risks |
| --- | --- |
| `Cargo.toml` | Seven-crate workspace, common metadata/dependencies, release profile. All crates inherit version/edition/license; version changes require lock/release consistency. |
| `Cargo.lock` | Exact resolved workspace/transitive dependencies. Used by locked builds; not a package repository lock for C++ projects. |
| `build-tools.toml` | Embedded bootstrap tool versions, URLs, SHA-256 values, archive selectors/executables. Read by catalog `sync`; library recipes do not belong here. |
| `.cargo/config.toml` | Static Microsoft CRT flag for x64 MSVC. Affects executable portability and build identities. |
| `.gitignore`, `.gitattributes` | Generated-output exclusions and line-ending policy. Never use ignore rules as an access-control mechanism. |
| `install.ps1` | User-facing prebuilt/offline bootstrap; executes the verified installer or colocated binary and updates its current process PATH. |
| `scripts/build.ps1` | Cargo build wrapper, discovers target output through metadata, optionally delegates developer install. |
| `scripts/install.ps1` | Developer copy installer; default LocalAppData location plus compatibility copy into `.local/bin`. Distinct from native setup's default. |
| `scripts/package.ps1` | Windows x64 MSVC host check, optional build, setup/ZIP/checksum generation and temporary staging cleanup. |
| `.github/workflows/ci.yml` | Windows formatting, Clippy, and build checks. No test-suite execution step. |
| `.github/workflows/release.yml` | Tag-driven build, artifact transfer, draft upload and stable/latest publication. Manual execution builds artifacts without publishing. |
| `README.md`, `overview.md`, `docs/*` | Usage and architecture policy; historical release notes must be read with their version. |
| `LICENSE`, `NOTICE`, `AUTHORS.md`, `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md` | Licensing, attribution, security reporting, contribution expectations, and stated history. Not runtime controls. |

All root paths above are relative to the repository. The [coverage matrix](COVERAGE_MATRIX.md) links each actual file.

## CLI crate: lifecycle and orchestration

[`main.rs`](../../crates/cpuv-cli/src/main.rs) defines `Cli`, `Action`, `SelfAction`, project/lock models, and orchestration. `main` selects setup, shim, or ordinary CLI mode. `execute` dispatches commands. `init`, `add`, and `sync` own project lifecycle and MSYS2 staging. `project_lock`, `write_atomic`, and `configure_environment` are critical helpers. Reads manifests, locks, compiler state and indexes; writes project state, shim, activation, and reports. It calls every major library and catalog/runner module. Changing its schemas, root-update logic, commit ordering, or dispatch affects both providers and command compatibility.

[`source.rs`](../../crates/cpuv-cli/src/source.rs) is the generic Conan adapter despite its historical filename. `Profile`, `CatalogLock`, `configure_profile`, `reference`, `modules_for`, `prepare_metadata`, `sync`, `commit`, and `requests_locked` translate portable roots/profile inputs into an external install and runtime interface. It imports CLI schemas, invokes tool/bootstrap helpers, linking, inspection and doctor, and records provider graphs/logs. Do not assume `source` means forced compilation: normal catalog mode reuses compatible binaries and builds missing ones. Upstream graph/property schema and transaction changes are high-risk modification points.

[`source/archive.rs`](../../crates/cpuv-cli/src/source/archive.rs) streams and hashes bootstrap archives, safely strips/selects paths, extracts ZIP/tar.gz, and inventories/verifies files. It calls first-party transfer policy and path validation. All currently pinned bootstrap tools are ZIPs; tar.gz remains a generic implementation branch. [`source/tools.rs`](../../crates/cpuv-cli/src/source/tools.rs) verifies keyed extracted tools, races safely when another profile commits the same tool, and concurrently prepares Conan/CMake/Ninja. Changes affect tool provenance and offline startup.

[`runner.rs`](../../crates/cpuv-cli/src/runner.rs) owns hidden state, command classification, output planning, build locks, runtime PATH, diagnostic streaming, execution, and legacy artifact archiving. `plan` must distinguish flags with values from source inputs and preserve compiler queries; unknown flag combinations are a major compatibility risk. Reads runtime metadata and invokes the real compiler directly rather than executing the installed shim for managed compilation.

[`host_tools.rs`](../../crates/cpuv-cli/src/host_tools.rs) implements `Manager` discovery and explicit `base-devel` pacman installation. It has host side effects outside project rollback. [`install.rs`](../../crates/cpuv-cli/src/install.rs) implements setup-name detection, native file installation, console retention and user-registry PATH handling through RAII-managed keys. [`update.rs`](../../crates/cpuv-cli/src/update.rs) implements release version/checksum parsing, bounded downloads, candidate execution, update lock and self replacement. These are executable trust and OS-permission boundaries.

## Supporting crates

[`cpuv-toolchain/src/lib.rs`](../../crates/cpuv-toolchain/src/lib.rs) defines `AbiTuple`, `Provider`, and `Toolchain`, compiler discovery/probing, driver-path normalization, language ceiling and `doctor`. Invoked by CLI initialization/sync/toolchain/doctor commands; results feed provider selection and catalog keys. [`msys.rs`](../../crates/cpuv-toolchain/src/msys.rs) discovers roots through environment, bounded directory markers, registry uninstall entries and App Paths. Discovery probes candidates and rejects ambiguity. Never replace observed capability checks with an installation-name heuristic.

[`cpuv-solver/src/lib.rs`](../../crates/cpuv-solver/src/lib.rs) parses `%FIELD%` MSYS2 records, normalizes namespaces, splits requirements, compares pacman-style versions, ranks suggestions, selects virtual providers and walks the graph. It has no network/storage side effects. Fetching and linking call its parsing/comparison helpers; the CLI calls availability/resolution. A comparison or provider change can affect lock validity and conflict detection across the whole graph.

[`cpuv-fetch/src/lib.rs`](../../crates/cpuv-fetch/src/lib.rs) implements `Fetcher`, `DownloadStats`, index caching/parsing, buffered downloads and streaming archive verification. [`policy.rs`](../../crates/cpuv-fetch/src/policy.rs) centralizes limits, loopback/HTTPS validation, client redirects and timeouts. These operate on untrusted remote metadata/bytes and are called by MSYS2, tool-bootstrap, and updater policy consumers. Index buffers are bounded but may be cloned during parsing.

[`cpuv-store/src/lib.rs`](../../crates/cpuv-store/src/lib.rs) defines file/package records, safe paths, package locks, extraction, per-file CAS ingestion, cached-content verification and materialization. It reads verified archives and writes cache records/blobs and private project stages. Symlink semantics, Windows path collisions, immutable hardlinks, and partial ingestion failures need careful review before changes.

[`cpuv-relocate/src/lib.rs`](../../crates/cpuv-relocate/src/lib.rs) defines runtime `Environment`, metadata relocation, specs generation, query detection, profile validation, and `compiler_args`. [`linking.rs`](../../crates/cpuv-relocate/src/linking.rs) parses public pkg-config metadata and ownership into one link plan. [`inspect.rs`](../../crates/cpuv-relocate/src/inspect.rs) parses PE binaries, selected archive members, and packaged named-symbol edges. These are shared by both providers and runner/shim; changes must preserve provider-specific origin semantics and avoid overstating inspection coverage.

[`cpuv-shim/src/lib.rs`](../../crates/cpuv-shim/src/lib.rs) locates an environment from executable adjacency or current-directory ancestors, obtains `compiler_args`, invokes the recorded compiler, and recursively deploys known imported DLLs beside raw outputs. [`main.rs`](../../crates/cpuv-shim/src/main.rs) is a minimal error/exit wrapper for the optional separate binary. The primary CLI links this library and dispatches to it when copied under a compiler name. The raw shim is not the managed runner and does not hide arbitrary GCC outputs.

Every crate has a `Cargo.toml` declaring its direct dependencies and inherited metadata; all are `publish = false`. Production status is IMPLEMENTED, with limitations and untested paths in [quality](11_TESTING_AND_QUALITY.md). The machine-readable [source inventory](source-inventory.json) records baseline hashes/line counts, not an assertion that every possible input was verified.
