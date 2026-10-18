# Coverage matrix and audit boundary

[Index](MASTER_INDEX.md) · [Reference symbols](21_REFERENCE_CATALOG.md) · [Source inventory](source-inventory.json)

## Snapshot

Verified 2026-10-17. Git base: `4889f1f96c527e45224b817be4aa4e0d8f5d0e17`; branch `main`; application 0.0.2 in a dirty working tree. Baseline captured after the explicitly requested rm alias and before encyclopedia generation. The source baseline contains 52 present files, including 19 Rust sources (6,003 physical lines), seven crate manifests plus workspace/tool/config TOML, four PowerShell scripts, two workflows, existing docs and license/attribution files. Deleted recipes.toml belongs to the earlier catalog work and is not counted as a present file.

## Coverage meaning

**Source reviewed** means component responsibilities, important symbols, inputs/outputs, and critical execution/validation/error branches were inspected. It does not mean every path, generated macro, dependency implementation, or possible input was executed. **Configuration reviewed** means declarations/defaults/dependency or workflow effects were inspected. **Policy/history reviewed** means content was used for current/historical claims rather than as proof that a feature runs. Line counts and hashes are exact baseline inventory facts.

| Baseline file | Lines | Review scope | Documentation chapters |
| --- | --- | --- | --- |
| [.cargo/config.toml](../../.cargo/config.toml) | 3 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [.gitattributes](../../.gitattributes) | 7 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [.github/workflows/ci.yml](../../.github/workflows/ci.yml) | 30 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [.github/workflows/release.yml](../../.github/workflows/release.yml) | 66 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [.gitignore](../../.gitignore) | 5 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [AUTHORS.md](../../AUTHORS.md) | 9 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [CHANGELOG.md](../../CHANGELOG.md) | 32 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [CONTRIBUTING.md](../../CONTRIBUTING.md) | 21 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [Cargo.lock](../../Cargo.lock) | 2193 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [Cargo.toml](../../Cargo.toml) | 43 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [LICENSE](../../LICENSE) | 201 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [NOTICE](../../NOTICE) | 11 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [README.md](../../README.md) | 287 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [SECURITY.md](../../SECURITY.md) | 7 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [build-tools.toml](../../build-tools.toml) | 26 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-cli/Cargo.toml](../../crates/cpuv-cli/Cargo.toml) | 41 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-cli/src/host_tools.rs](../../crates/cpuv-cli/src/host_tools.rs) | 77 | Source reviewed; dynamic coverage bounded | 02, 06, 08 |
| [crates/cpuv-cli/src/install.rs](../../crates/cpuv-cli/src/install.rs) | 264 | Source reviewed; dynamic coverage bounded | 08, 09, 10, 12 |
| [crates/cpuv-cli/src/main.rs](../../crates/cpuv-cli/src/main.rs) | 1066 | Source reviewed; dynamic coverage bounded | 03, 06, 07, 08, 12, 15 |
| [crates/cpuv-cli/src/runner.rs](../../crates/cpuv-cli/src/runner.rs) | 481 | Source reviewed; dynamic coverage bounded | 06, 08, 12, 15, 18 |
| [crates/cpuv-cli/src/source.rs](../../crates/cpuv-cli/src/source.rs) | 808 | Source reviewed; dynamic coverage bounded | 03, 06, 07, 09, 12, 19 |
| [crates/cpuv-cli/src/source/archive.rs](../../crates/cpuv-cli/src/source/archive.rs) | 240 | Source reviewed; dynamic coverage bounded | 05, 07, 09, 14 |
| [crates/cpuv-cli/src/source/tools.rs](../../crates/cpuv-cli/src/source/tools.rs) | 99 | Source reviewed; dynamic coverage bounded | 04, 05, 06, 07 |
| [crates/cpuv-cli/src/update.rs](../../crates/cpuv-cli/src/update.rs) | 204 | Source reviewed; dynamic coverage bounded | 08, 09, 10, 12, 15 |
| [crates/cpuv-fetch/Cargo.toml](../../crates/cpuv-fetch/Cargo.toml) | 23 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-fetch/src/lib.rs](../../crates/cpuv-fetch/src/lib.rs) | 360 | Source reviewed; dynamic coverage bounded | 06, 07, 08, 14, 15 |
| [crates/cpuv-fetch/src/policy.rs](../../crates/cpuv-fetch/src/policy.rs) | 63 | Source reviewed; dynamic coverage bounded | 08, 09, 12, 15 |
| [crates/cpuv-relocate/Cargo.toml](../../crates/cpuv-relocate/Cargo.toml) | 19 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-relocate/src/inspect.rs](../../crates/cpuv-relocate/src/inspect.rs) | 151 | Source reviewed; dynamic coverage bounded | 03, 09, 14, 19 |
| [crates/cpuv-relocate/src/lib.rs](../../crates/cpuv-relocate/src/lib.rs) | 380 | Source reviewed; dynamic coverage bounded | 03, 06, 07, 13, 17 |
| [crates/cpuv-relocate/src/linking.rs](../../crates/cpuv-relocate/src/linking.rs) | 277 | Source reviewed; dynamic coverage bounded | 02, 06, 08, 14 |
| [crates/cpuv-shim/Cargo.toml](../../crates/cpuv-shim/Cargo.toml) | 15 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-shim/src/lib.rs](../../crates/cpuv-shim/src/lib.rs) | 161 | Source reviewed; dynamic coverage bounded | 03, 05, 06, 12 |
| [crates/cpuv-shim/src/main.rs](../../crates/cpuv-shim/src/main.rs) | 9 | Source reviewed; dynamic coverage bounded | 05, 06 |
| [crates/cpuv-solver/Cargo.toml](../../crates/cpuv-solver/Cargo.toml) | 14 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-solver/src/lib.rs](../../crates/cpuv-solver/src/lib.rs) | 382 | Source reviewed; dynamic coverage bounded | 02, 06, 14, 17 |
| [crates/cpuv-store/Cargo.toml](../../crates/cpuv-store/Cargo.toml) | 21 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-store/src/lib.rs](../../crates/cpuv-store/src/lib.rs) | 346 | Source reviewed; dynamic coverage bounded | 06, 07, 09, 14 |
| [crates/cpuv-toolchain/Cargo.toml](../../crates/cpuv-toolchain/Cargo.toml) | 20 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [crates/cpuv-toolchain/src/lib.rs](../../crates/cpuv-toolchain/src/lib.rs) | 422 | Source reviewed; dynamic coverage bounded | 04, 06, 07, 12, 17 |
| [crates/cpuv-toolchain/src/msys.rs](../../crates/cpuv-toolchain/src/msys.rs) | 213 | Source reviewed; dynamic coverage bounded | 05, 06, 12 |
| [docs/architecture.md](../../docs/architecture.md) | 109 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [docs/compatibility.md](../../docs/compatibility.md) | 44 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [docs/release-notes/v0.0.1.md](../../docs/release-notes/v0.0.1.md) | 37 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [docs/release-notes/v0.0.2.md](../../docs/release-notes/v0.0.2.md) | 7 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [docs/releases.md](../../docs/releases.md) | 55 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [docs/source-builds.md](../../docs/source-builds.md) | 64 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [install.ps1](../../install.ps1) | 67 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [overview.md](../../overview.md) | 75 | Policy/history/documentation reviewed | 01, 13, 19, 20 |
| [scripts/build.ps1](../../scripts/build.ps1) | 19 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [scripts/install.ps1](../../scripts/install.ps1) | 65 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |
| [scripts/package.ps1](../../scripts/package.ps1) | 45 | Configuration/tooling reviewed | 04, 05, 10, 12, 21 |

## Checks and consistency

The commands and six passing behavioral scenarios are recorded in [chapter 11](11_TESTING_AND_QUALITY.md). Final validation passed: all 22 requested chapters are present, all 52 baseline file hashes are unchanged, all 358 relative links resolve, lexical source-symbol locations match, fenced blocks are balanced, and no personal absolute paths were found in the generated Markdown. There are 24 Markdown documents including this matrix and the diagram collection, plus the machine-readable source inventory. Nine Mermaid blocks were structurally checked and manually reviewed. These are documentation/static checks, not rendered Mermaid verification or a comprehensive security audit. `git diff --check` also passed; Git emitted only its existing automatic line-ending notices for Cargo.lock and NOTICE.

## Explicit gaps

- Upstream Conan recipes, full Conan/Python internals, downloaded tool source, third-party Rust implementations and Windows internals were not exhaustively reviewed. Dependency versions and integration contracts were inventoried.
- No live GitHub issue/PR/release infrastructure or remote branch fetch was required for this local audit; local Git refs/history were inspected.
- No minimum Windows/Rust or arbitrary GCC/linker matrix was executed.
- No fault injection during failed rollback, concurrent-environment mutation, independent vulnerability scan, signature validation, coverage percentage, or controlled performance study was performed.
- Source APIs and important data models are documented; exact future provider JSON compatibility and arbitrary library build features remain upstream-dependent.
- The original technical PDF is not part of this repository baseline. Current source takes precedence over historical design claims.
- Mermaid source is structurally checked and manually reviewed; no external rendering service is claimed.

## Source change boundary

Only the new visible alias on Action::Remove was added to application source for this request. Earlier catalog changes were already present and preserved. Encyclopedia writing did not rewrite existing README, policy documents, dependency versions, source, or Git history. Ignored binaries/local installations were refreshed as delivery of the authorized feature; no publication occurred.
