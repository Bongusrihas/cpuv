# 11 · Testing and quality

[Index](MASTER_INDEX.md) · [Build instructions](10_BUILD_RUN_TEST_AND_DEPLOY.md) · [Roadmap](16_IMPLEMENTATION_STATUS_AND_ROADMAP.md)

## Retained infrastructure

**NOT FOUND in the source baseline:** unit/integration/end-to-end test directories, Rust test attributes, retained fixtures/mocks, coverage reports, property tests, fuzz targets, security test jobs, or benchmark programs. Existing [CONTRIBUTING](../../CONTRIBUTING.md) explicitly describes a source distribution excluding tests and simulations. This audit respects that distribution choice; it does not imply the behavior has never been exercised.

**IMPLEMENTED:** Windows CI runs rustfmt checking, Clippy with warnings denied, and a locked workspace build. Release CI repeats checks and builds prebuilt assets. Neither workflow runs a behavioral test suite. A successful build is not evidence that a resolver, rollback path, compiler driver, or provider adapter works for every input.

## Verification performed for this change

Date: 2026-10-17. Application source: 0.0.2 working tree based on `4889f1f`, with `rm` added as a visible alias of `Action::Remove`. Rust/Cargo: 1.99.0 on Windows. Existing local UCRT64 GCC: 16.2.0. Exact execution paths and external fixture implementation were inspected; no source test files were added to the repository.

| Check | Actual result | Limits |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Formatting only |
| `cargo clippy --workspace --locked -- -D warnings` | PASS | Static lint/build analysis, not behavioral coverage |
| `cargo build --workspace --release --locked` | PASS | Release workspace build with static CRT configuration |
| `cpuv --help`, `cpuv rm --help`, empty `cpuv rm` | PASS | Alias discoverable, help resolves, missing packages returns usage status 2 |
| Controlled MSYS2 roots alpha/beta sharing common | PASS | `rm alpha --offline` removed alpha while preserving beta/common |
| Transitive-only and mixed valid/invalid removal | PASS | Nonzero result; manifest, lock, active metadata hashes unchanged |
| Existing `remove` command and last MSYS2 root | PASS | Empty project payload; archives retained |
| Cached Conan roots xtensor and nlohmann_json | PASS | Transitive-only removal rejected; removing xtensor retained the declared JSON root |
| Compile/run after catalog removal | PASS | Remaining JSON dependency compiled and emitted `{"kept":true}` |
| Last catalog root removal | PASS | Empty catalog environment committed successfully offline |
| Developer installation and installed `rm --help` | PASS | Both existing local executable locations updated; PATH left unchanged |
| `scripts/package.ps1 -SkipBuild` | PASS | Local release assets regenerated; not uploaded |

The removal harness grouped these behavioral checks into six passing scenarios. Its MSYS2 HTTP server bound a loopback port selected by the OS, generated small snapshot/archive payloads externally, and shut down afterward. Conan checks used a real populated local provider cache with remotes disabled. Failures were expected where explicitly testing validation. This is targeted evidence, not a coverage percentage or universal provider certification.

Local diagnostic artifacts were retained outside the repository under the audit artifact directory: `rm-results.json` and `rm-commands.log`. They are not required runtime inputs or portable source tests. Source-hash consistency and documentation-link validation are recorded in [coverage](COVERAGE_MATRIX.md).

## Earlier evidence and replay limits

Earlier work in the same development session exercised xtensor, compiled fmt/zlib, ABI-0 and strict C++20 profiles, UCRT/MSVCRT compilers, offline reuse, cache corruption rejection, and managed output behavior. Those results help explain the current implementation but were not all rerun for this alias-only change. No Qt-specific benchmark or fabricated timing/throughput result is incorporated. Upstream Conan recipes and downloaded tools are not validated by cpuv's CI.

## Gaps and recommended strategy

P1: preserve executable regression cases somewhere reproducible for contributors, even if distributed source remains test-free. Highest-priority cases are malformed archives/metadata, rollback under Windows file locks, altered cached metadata, catalog nested archive inspection, and concurrent build/update behavior. P2: compare resolver/version semantics with authoritative pacman cases, add compiler/GCC/runtime matrices, test provider JSON/schema changes and component interfaces, and verify fresh-machine native installation. P3: introduce controlled cold/warm performance studies and minimum-Rust/OS validation.

These are **recommendations**, not existing jobs. No code coverage number, flaky-test rate, independent vulnerability scan, API contract suite, or published benchmark dataset is available from this checkout. Assertions about absent suites are limited to inspected first-party paths, not historical private/deleted development artifacts.
