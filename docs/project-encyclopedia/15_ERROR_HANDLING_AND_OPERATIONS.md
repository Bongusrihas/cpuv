# 15 · Error handling and operations

[Index](MASTER_INDEX.md) · [Security](09_SECURITY_AND_TRUST_MODEL.md) · [Known issues](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md)

## Error conventions and observability

Most functions return `anyhow::Result` and attach context at subsystem boundaries. `main` prints `error: {error:#}` and exits 1 for application failures. The shim has its own `cpuv-shim:` wrapper. Clap owns parsing/help errors, including missing required roots. Compiler/program failures return their child exit code; absent status maps to 1. There is no structured stable error-code taxonomy, HTTP error body, log-level hierarchy, telemetry collector, metrics endpoint, alert service, or health daemon.

MSYS2 prints transfer progress and final counts/timings. `.cpuv/last-sync.json` records operation diagnostics; it is not monitoring or a benchmark. Catalog subprocesses append complete stdout/stderr and debug command descriptions to cache logs; failures include the final 28 lines and log path. `CPUV_SHIM_TRACE` prints compiler invocations. Logs can reveal local paths and arbitrary user-provided flags. No log rotation/redaction subsystem was found.

## Retries and timeouts

The first-party archive/index client retries up to three times with short exponential backoff. Shared client defaults are 15 seconds connect, 60 seconds idle read, and 3,600 seconds whole request; zero disables only the whole deadline. Index/size/unsafe redirect failures can be retried but do not bypass validation. Tool bootstrap uses three attempts and its own 150 ms stepped backoff. Updater asset downloads use three attempts with a 180-second request timeout and fixed payload caps. Provider processes, compiler probes, builds and programs do not receive a universal cpuv subprocess timeout.

There is no explicit workflow cancellation/checkpoint protocol. OS termination can leave staging/recovery directories or partial project initialization. RAII normally releases scoped file locks and temporary objects, but multi-file swaps are not crash-proof. Do not assume an interrupted operation rolls back like a database transaction.

## Operational playbook

| Failure | Observed path and recommended response |
| --- | --- |
| Compiler not found/ambiguous | Discovery errors before provider install. Check PATH, CPUV_CXX/CXX, or use `--compiler`; select the real driver rather than `.cpuv` shim. |
| Compiler compiles but cannot run | `doctor` executes a probe and reports runtime/architecture failure. Check compiler DLL paths and target compatibility. |
| Missing package/version | MSYS2 absence can move the whole graph to Conan; unsupported catalog names/features fail with provider logs. Verify exact ecosystem name and available version. |
| Missing cached input offline | Fetch/tool/provider refuses required remote retrieval. Restore the cache or run intentionally online; offline is not an upstream subprocess sandbox. |
| Archive mismatch/truncation | SHA/length validation refuses extraction/publication. Inspect mirror/cache state rather than disabling checks. |
| Unsafe archive payload | Traversal/prefix/device/special-entry checks abort stage preparation. Treat as unsupported/malformed input. |
| Modified hardlinked payload | `Store::cached` errors rather than trusting the altered package. Preserve diagnostics; use a fresh cache or explicit repair procedure. There is no general repair command. |
| Missing header after successful install | Compiler diagnostics plus generic guidance; compare the actual installed header path/version and include directories. No automatic guessed package installation occurs. |
| Missing link module after `rm` | Remaining explicit `[link]` points to an absent package; operation fails before commit. Update the intentional interface selection and retry. |
| Non-direct package removal | `sync` rejects the key before saving state. Remove the direct root that requires it, if that is intended. |
| File/process lock conflict | Project/build/catalog/update locks return scoped errors; package locks can wait. Stop conflicting work and retry; program/DLL Windows locks may also block renames. |
| Environment commit error | Ordinary rollback attempts restoration. Catalog keeps backups if restoration fails; MSYS2 has weaker recovery retention. Preserve any surviving stage/backup before manual repair. |
| Setup/update destination locked | Installation/replacement reports the failure. Close relevant processes and retry; never overwrite unrelated executables or assume every installed copy updated. |

## Transaction boundaries and partial success

MSYS2 stages environment/manifest/lock, swaps them, then writes toolchain JSON, activation, and report. A post-swap error can therefore mean dependencies changed despite a nonzero command result. Catalog includes toolchain/activation in its five-artifact commit but writes its report afterward; report failure is also post-commit. Shim installation occurs before each backend's main swap. Explicit host pacman installation is separate and not rolled back with project files.

`clean` persists its archive before moving loose files. If a move fails, already moved files remain preserved in that archive and some source-root files may remain; it is not an all-or-nothing move. `rm` rebuilds package reachability and retains cache bytes. No project backup scheduler, automatic cache pruning, or native application self-uninstall workflow exists.

## Diagnosis without modifying the project

Start with `--help`, `toolchain`, and existing metadata/log files. `doctor` safely generates a temporary program but does execute the selected compiler and resulting binary. `sync` is a mutation, even if often used as a repair step; do not call it merely to inspect state. Avoid printing credential values or publishing arbitrary logs without reviewing flags/paths. There are no secret-management guarantees for user-supplied values.

Sources: [CLI](../../crates/cpuv-cli/src/main.rs), [catalog](../../crates/cpuv-cli/src/source.rs), [runner](../../crates/cpuv-cli/src/runner.rs), [policy](../../crates/cpuv-fetch/src/policy.rs), [tools](../../crates/cpuv-cli/src/source/archive.rs), [updater](../../crates/cpuv-cli/src/update.rs).
