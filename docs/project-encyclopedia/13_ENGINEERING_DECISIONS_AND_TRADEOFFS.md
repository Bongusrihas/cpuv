# 13 · Engineering decisions and trade-offs

[Index](MASTER_INDEX.md) · [Architecture](03_ARCHITECTURE_DEEP_DIVE.md) · [Issues](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md)

No historical ADR directory was found. These are reconstructed decision records. **Documented** indicates explicit existing policy/comments/changelog; **inferred** indicates likely motivation from implementation; **recommendation** indicates a new audit proposal. Alternatives are newly evaluated unless history explicitly establishes them.

## D1 · Rust workspace with one main executable

Decision/evidence: [Cargo.toml](../../Cargo.toml) defines seven Rust crates; `main` links shim/setup paths and dispatches by executable name. Documented purpose: standalone installation without user Rust. Inferred benefits: typed component boundaries, in-process calls, and one primary runtime artifact. Costs: release linking time, Windows FFI maintenance, name-dispatch coupling, and Rust prerequisites for contributors. Alternative: separate CLI/daemon/helpers, introducing more installation/version coordination. Revisit if background work or truly independent service lifecycles become requirements.

## D2 · One validated binary ecosystem first

Decision/evidence: `parse_verbose` makes the managed binary provider eligible only for recognized MSYS2 UCRT64 x64 POSIX/SEH, dual ABI 1; [compatibility policy](../compatibility.md) explicitly bounds support. Documented rationale: compiler names/hashes do not guarantee arbitrary binary compatibility. Benefits: constrained metadata/layout/runtime expectations. Costs: packages or compilers outside the boundary need another path. Alternative: additional explicit adapters, each with its own validated layout/profile. Revisit after correctness and reproducible compatibility tests, not by widening a name predicate alone.

## D3 · Shared ConanCenter metadata replaces cpuv library recipes

Decision/evidence: `source::sync` invokes external Conan, and `build-tools.toml` contains tools only. Existing contribution/changelog policy says not to add library-name branches or recipes. Benefits: upstream transitive requirements and maintained build instructions; broad automation without a first-party recipe per library. Costs: upstream recipe execution, schema/version coupling, provider tooling/bootstrap, external network behavior and incomplete catalog coverage. Alternative: maintain an independent recipe catalog or add another ecosystem adapter. Revisit when concrete packages/platforms justify an adapter and its trust model.

## D4 · Do not mix provider graphs

Decision/evidence: main `sync` routes the entire manifest to Conan when a root is absent or profile is ineligible. Inferred rationale: avoid unsupported runtime/layout/ABI combinations across catalogs. Benefits: one resolver/profile authority per environment. Costs: a valid MSYS2 name may be invalid in Conan; fallback can fail for a mixed set of names. Alternative: package-level federation with explicit binary compatibility and metadata translation, substantially more complex. Revisit only with a defined cross-provider ABI contract.

## D5 · File CAS and hardlink materialization

Decision/evidence: `Store::ingest`, `cached`, `materialize` store BLAKE3 blobs and per-archive records. Documented goal: shared immutable reuse with copy fallback. Benefits: storage deduplication and cheap same-volume materialization. Costs: hash verification and immutable-link discipline; accidental edits can affect multiple environments. Alternative: private copies/reflinks or a database-backed package store. Revisit if cross-volume costs, integrity recovery, or cache retention policies dominate measured workloads.

## D6 · Package metadata selects links

Decision/evidence: `linking::apply` consumes ownership, public `.pc` interfaces, and explicit `Selection`; diagnostic include maps select no libraries. Documented rationale: includes do not reliably identify transitive/system link dependencies. Benefits: repeatable object-only/conditional-header behavior. Costs: bounded parser semantics and explicit work for ambiguous modules; no arbitrary CMake target evaluation/private static interfaces. Alternative: evaluate provider-native CMake targets or require explicit user flags. Revisit when public metadata cannot represent a supported real use case.

## D7 · Shim primary; GCC specs experimental

Decision/evidence: `install_shim`, `compiler_args`, and `generate_specs`; catalog specs explicitly direct users to the shim. Documented rationale: merely creating a specs file does not make GCC load it. Benefits: controlled arguments and diagnostics without modifying GCC installation. Costs: wrapper startup, metadata contract, raw/managed path differences. Alternative: verified explicitly loaded specs or toolchain-native integration. Revisit only with experimental measurements and clear loading semantics.

## D8 · Profile identity as a compatibility filter

Decision/evidence: `AbiTuple`, broader catalog key, saved flags and binary reports; reports explicitly deny a universal guarantee. Benefits: avoid obvious mismatches and rebuild with matching chosen inputs. Costs: incomplete feature/debug/ISA/LTO information and external compiler-support changes not fully captured. Alternative: complete provenance plus symbol/interface compatibility testing, still not a mathematical universal ABI proof. Revisit the identity when a demonstrated collision or missing build input appears.

## D9 · Bounded asynchronous downloads and separate extraction jobs

Decision/evidence: `buffer_unordered(concurrency)` and extraction limit 4; shared timeout/size policy. Inferred rationale: constrain simultaneous network/CPU/disk pressure. Benefits: parallel throughput with backpressure. Costs: fixed extraction parallelism and synchronous provider/build waits. Alternative: adaptive weighted scheduling, requiring measurement and more state. Revisit after workload-specific profiling; do not assume the default 50 means 50 active transfers.

## D10 · Staged filesystem transactions

Decision/evidence: both `sync` implementations stage artifacts then rename with backups. Benefits: preparation failures usually preserve the active environment. Costs: multiple files are not atomically committed together; binary/catalog algorithms differ and rollback failures remain possible. Alternative: generation directories plus one atomic pointer switch/journaled transaction abstraction. **Recommendation:** consolidate recovery logic and retain backups on all restoration failures before broadening transactional claims.

## D11 · Native per-user install with optional shell bootstrap

Decision/evidence: native registry/file APIs in `install.rs`, static CRT config, optional `install.ps1`. Documented goal: install prebuilt cpuv without Rust, admin access, or a required PowerShell command. Benefits: simple user deployment. Costs: executable reputation/signing concerns, PATH propagation, duplicate local copies. Alternative: Windows package manager/MSIX integration. Revisit for signed distribution and OS-native uninstall/installation inventory.

## D12 · Excluded retained tests

Decision/evidence: contribution policy and source inventory; CI builds/lints but no behavioral suite. Original underlying rationale is not established beyond the stated distribution policy. Benefits may include a smaller source-only distribution, but that benefit is an inference. Costs: weak reproducibility and regression visibility for contributors. **Recommendation:** provide an explicitly maintained external or optionally included regression suite; do not describe compilation as functional coverage.

## D13 · `rm` aliases existing removal

Decision/evidence: `Action::Remove` now has `visible_alias = "rm"`; execution still calls the same `sync` removal path. Explicit current requirement: provide `cpuv rm`. Benefits: preserves old command compatibility and avoids duplicating uninstall logic. Costs: help usage retains the canonical `remove` name. Alternative: rename the canonical command with a backward alias. Revisit only if a stable CLI naming policy requires that presentation change; no runtime behavior change is needed.
