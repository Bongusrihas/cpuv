# 10 · Build, run, verify, and deploy

[Index](MASTER_INDEX.md) · [Quality results](11_TESTING_AND_QUALITY.md) · [Configuration](12_CONFIGURATION_AND_ENVIRONMENT.md)

## Prerequisites and preparation

For prebuilt use, install cpuv on x64 Windows and supply a working native GCC for C++ compilation. Native setup requires no Rust or administrator account by design. The documented OS floor is Windows 10; this audit did not exercise that entire OS matrix. Initial package/tool retrieval needs network access; a complete cache can support offline reuse.

For development, install stable Rust at or above the declared 1.88 minimum and the Windows MSVC build tools needed by Rust's x64 MSVC target. Actual successful checks here used rustc 1.99.0 and Cargo 1.99.0, not the minimum toolchain. Clone/open the source root before invoking Cargo so `.cargo/config.toml` supplies the static CRT flag. Package scripts specifically check an x86_64-pc-windows-msvc Rust host.

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --locked -- -D warnings
cargo build --workspace --release --locked
```

**VERIFIED:** all three completed successfully after adding the alias. They are checks/builds, not evidence of a retained automated test suite. Optional `CARGO_TARGET_DIR` can keep outputs outside the source folder; `scripts/build.ps1` and packaging discover that directory with Cargo metadata.

## Run and remove dependencies

```powershell
cpuv init
cpuv add fmt
cpuv run g++ program.cpp
cpuv rm fmt
```

The individual commands are implemented; the sequence above is a reusable example rather than a claim that every library version was installed during this audit. Actual removal validation used both a controlled MSYS2 repository and cached ConanCenter packages. The exact checks are in [chapter 11](11_TESTING_AND_QUALITY.md).

Use `cpuv --catalog add PACKAGE` to choose ConanCenter for the whole graph. Use exact catalog names. `cpuv sync --offline` and `cpuv rm --offline PACKAGE` work only when remaining graph/tool inputs are cached. Compiler output and program exit status propagate. `run --no-run g++` builds without execution. Non-program compiler modes do not launch an executable. A command from a different directory should use `cpuv -C PROJECT ...`; ordinary CLI manifest lookup is not an ancestor search.

## Component development and debugging

Cargo package names are `cpuv-cli`, `cpuv-toolchain`, `cpuv-solver`, `cpuv-fetch`, `cpuv-store`, `cpuv-relocate`, and `cpuv-shim`. Building a single package with `cargo build -p NAME --locked` follows workspace configuration; this per-package form is inferred from manifests rather than separately executed for every crate. Most components are Rust libraries, not independent services. The separate `cpuv-shim` executable requires a discoverable environment.

Use `cpuv toolchain` for JSON observations and `cpuv doctor -- DRIVER_FLAGS` for a real compiler/linker/runtime probe. Set `CPUV_SHIM_TRACE` to display recorded compiler invocations; any present value enables tracing. Read `.cpuv/env/link-plan.json`, `compatibility.json`, and `.cpuv/last-sync.json`. Catalog failures show a cache log path with complete provider output. There is no tracing collector, debug web UI, database setup, or migration command.

## Tests and validation

The repository intentionally excludes retained test suites/fixtures in its existing contribution policy. No `cargo test` result is presented as meaningful coverage. Developer verification for this change used an external harness, so the checkout remains free of test artifacts. Reconstructing regression cases is described in [chapter 18](18_CONTRIBUTOR_GUIDE_AND_EXTENSION_MANUAL.md); quality gaps and priorities are explicit in [chapter 11](11_TESTING_AND_QUALITY.md).

## Packaging, installation, and publication

```powershell
.\scripts\package.ps1
# After an existing release build:
.\scripts\package.ps1 -SkipBuild
```

The skip-build form was **VERIFIED** with the external Cargo target directory. Outputs: setup executable/checksum, portable Windows ZIP/checksum, and PowerShell bootstrap under ignored `dist`. The ZIP contains cpuv.exe, install.cmd, install.ps1, README, LICENSE, and NOTICE; it is not a complete source archive or this encyclopedia. The native installer and portable executable are the same application with name-based dispatch.

`scripts/install.ps1 -BinaryDirectory RELEASE_DIRECTORY -NoPath` was **VERIFIED** for updating installed local copies without changing user PATH. It differs from public native setup: its default destination is LocalAppData and its compatibility copy is `.local/bin`. Native `cpuv self install` defaults to `.local/bin`. Installed `cpuv rm --help` was also verified. No user project dependency was removed as a test.

The release workflow checks source, builds, packages, transfers Actions artifacts, then publishes tag-triggered assets through a draft to stable/latest. A manual workflow run does not publish. Publication was not run during this audit; no commit/tag/push or remote release modification occurred. Follow [existing release instructions](../releases.md) when intentionally publishing a new version.

## Upgrade, shutdown, and common failures

`self update` changes only the invoked application copy after release checks; it preserves caches/projects. It is implemented but not re-executed against live release infrastructure here. Existing source locks migrate during successful catalog sync. Raw compiler processes and programs end normally; there is no daemon shutdown command. `clean` archives loose artifacts rather than clearing caches. `rm` removes dependencies, not cpuv itself; no native `self uninstall` command was found.

For missing headers, verify the actual installed header layout before installing unrelated packages. For a missing link module after removal, update stale `[link]` selections. For corrupted hardlinked cache files, preserve diagnostics and use a fresh cache/rebuild; do not edit shared payload in place. For locked executables or environments, stop the relevant program before retrying. See [operations](15_ERROR_HANDLING_AND_OPERATIONS.md).

Sources: [manifests](../../Cargo.toml), [build wrapper](../../scripts/build.ps1), [package wrapper](../../scripts/package.ps1), [developer installer](../../scripts/install.ps1), [CI](../../.github/workflows/ci.yml), [release workflow](../../.github/workflows/release.yml).
