# 18 · Contributor and extension manual

[Index](MASTER_INDEX.md) · [File guide](05_REPOSITORY_AND_FILE_BY_FILE_GUIDE.md) · [Decisions](13_ENGINEERING_DECISIONS_AND_TRADEOFFS.md)

## Development workflow

Read the current [compatibility policy](../compatibility.md), [contribution guide](../../CONTRIBUTING.md), and architecture/walkthrough before editing. Inspect `git status` first: this snapshot includes substantial uncommitted catalog work, so a clean HEAD is not the complete implementation under discussion. Preserve unrelated changes. Build with the workspace's Windows MSVC/static-CRT configuration and `--locked`; use an external Cargo target directory if generated files should stay outside the checkout.

Run formatting, Clippy with warnings denied, a workspace build, and targeted behavioral checks appropriate to the change. This checkout intentionally excludes retained tests; do not add fixtures/results/benchmarks silently. If that policy changes, arrange an explicit reproducible regression location and wire it into CI. Do not claim a zero-test `cargo test` result measures behavior.

## Concrete extension examples

### A compatible command alias

The current change adds `#[command(visible_alias = "rm")]` to `Action::Remove`. `execute` continues to call the existing removal `sync` path. This is a parser/API extension rather than another uninstaller. Verify help, missing arguments, old spelling, direct-root removal, shared transitive dependencies, invalid multi-package atomicity, empty graphs, offline behavior, and preserved caches. Do not implement alias behavior twice.

### A new provider or catalog option

Add an explicit metadata/layout/profile adapter rather than an `if library_name == ...` branch. Establish who resolves dependencies, owns files, supplies public interfaces, determines binary compatibility, performs integrity checks, and commits the environment. Define name/version semantics and whether graph mixing is permitted; current behavior is one provider per environment. Preserve the selected compiler and avoid substituting foreign ABI binaries just because a download exists.

For a generic catalog option, first define manifest/lock representation and package-ID/cache implications. Update `configure_profile` or provider arguments, runtime interface generation, validation, help/docs, and failure cases together. The current detailed linkage label is not a complete provider option mechanism. Upstream package build fixes belong in upstream catalog metadata, not a revived cpuv recipe registry.

### A schema or runtime metadata change

Trace `Manifest`, version-1 `Lock`, `CatalogLock`, `Environment`, `Selection`, and all readers. Distinguish portable inputs from machine-local paths. Define an explicit version/migration strategy and negative cases for unsupported versions. Changes to compiler argument data affect both managed runner and raw shim. Preserve query behavior and provider-specific profile guards; do not conflate user-facing backend `conan` with internal origin `source`.

### A new compiler output option

Extend `runner::takes_value`/`plan` only after understanding driver token consumption. Keep relative input paths rooted in the project while rewriting outputs into `.cpuv/build`. Preserve compile-only/query/shared/relocatable modes and application arguments after `--`. Check output basename collisions, auxiliary files, failure-before-execution, and program exit propagation. Response files require a real parser/design; simply forwarding them would bypass current output/profile assumptions.

## Configuration, dependencies, and contracts

CLI options live in `Cli` or command-specific structs. Use typed defaults/ranges and document environment precedence. Update the full configuration reference and relevant locks/cache keys when an option affects build identity. Bootstrap tool pins belong in `build-tools.toml`, including official URLs/digests and executable/archive filters. Rust dependency constraints belong in workspace/per-crate manifests with Cargo.lock updated intentionally. Do not change versions during a documentation-only audit.

Public Rust interfaces are function/type boundaries, not network endpoints. Extend existing component ownership instead of placing HTTP, storage, and compiler logic into the solver. If a real network API is proposed, specify its authorization, request/error schema, lifecycle, and threat model explicitly; none exists now.

## Debugging and review

Use toolchain JSON and `doctor` for driver problems; metadata/link-plan/compatibility reports for interface problems; provider logs for catalog builds. Preserve failing stages/backups before manual recovery. Review logs before sharing. For corruption checks, use disposable caches and restore fixtures; never modify an active user's shared payload to reproduce a bug.

Review questions: Does the change preserve source ownership and compatibility boundaries? Are portable locks free of personal paths? Do paths/collisions/redirects remain validated? Are caches still immutable? Does failure preserve the previous environment, including rollback failure? Are build/use locks coordinated? Does the implementation actually support the declared interface, not merely parse it? Is new rationale labeled as recommendation rather than invented history?

Sources: [CLI](../../crates/cpuv-cli/src/main.rs), [catalog](../../crates/cpuv-cli/src/source.rs), [runner](../../crates/cpuv-cli/src/runner.rs), [compiler arguments](../../crates/cpuv-relocate/src/lib.rs), [link selection](../../crates/cpuv-relocate/src/linking.rs), [CI](../../.github/workflows/ci.yml).
