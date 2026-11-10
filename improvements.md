# cpuv 0.0.6 implementation tracking

## 0.0.6 implementation status

The review below is a historical 0.0.5 baseline. Version 0.0.6 implements B01–B10 and addresses the publication/identity gaps R01–R03. Larger product features remain on the roadmap, following the agreed maintenance-release scope.

| Review items | 0.0.6 outcome |
| --- | --- |
| B01 | Detailed dependency edits preserve options/linkage/reference; incompatible source/reference edits fail explicitly. |
| B02 | All declared tools and interpreter launchers are prepared for CMake and other project adapters. |
| B03 / B06 | Native adapter paths include runtime search directories; generic run inherits the selected developer environment. |
| B04 / B05 | Explicit CMake standard mapping/default reset and stricter validation of profile-owned settings. |
| B07 | Explicit provider selection survives profile creation; final vcpkg combinations are validated. |
| B08 | Raw shim uses bounded PE mappings, streaming digests, and an indexed DLL candidate set. |
| B09 | Canonical artifact identities and verified legacy snapshot discovery. |
| B10 | Unchanged source copies preserve timestamps; changed/missing copies are repaired. |
| R01 | Catalog locks record compiler/SDK/bootstrap inputs and host/build binary graph identities. Legacy provider rebuilds require explicit lock migration. |
| R02 | vcpkg preparation acquires a per-revision bootstrap lock for every caller. |
| R03 | Offline import holds the exclusive cache lease for its entire publication operation. |
| O02 / O06 / O09 / O10 | Incremental source copying, cached Requires regex/deterministic ready queue, early CLI parsing, and indexed/bounded DLL deployment. |
| O04 / O11 | Bootstrap transfer writes use async I/O; read-only cache inspection/preview uses a shared lease. |
| A01 / A02 / A04 / A05 | Dependency edit policy, shared declared-tool preparation/runtime path composition, canonical identities, provider graph checks, and stronger lock ownership implemented incrementally. |
| R04 / F01–F10 | Retained support/trust boundaries and future product roadmap. |
| O01 / O03 / O05 / O07 / O08 / O12 / remaining A items | Benchmark/design backlog: no unmeasured persistent hash shortcuts, graph-scoped integrity relaxation, remote storage model, or broad orchestration rewrite is claimed in this release. |

Local validation on 2026-11-10:

- Debug and optimized workspace suites: each 86 passed, 0 failed, 5 explicit/manual ignores.
- Clippy with warnings denied, formatting, and Rust 1.88 workspace/all-target checks: passed.
- Current RustSec dependency audit: passed (227 locked dependencies scanned).
- Optimized Phase 1 native MSYS2 gate: passed, including standalone shim, corruption repair, rollback, and reader/writer exclusion.
- Phase 2 source/provider gate: passed, including local/archive/Git/SDK/Make dependencies, generations, CMake tests, offline transfers and cache repair.
- Installer and portable ZIP packaged; executable reports cpuv 0.0.6.
- Local optimized MSVC Phase 3 consumer gate: in progress. Release automation independently runs the native acceptance gates before publishing assets.

The earlier Phase 1 run exposed the fixture's old raw-text artifact key expectation; it was updated to canonical identity and the complete native gate then passed.

Migration and behavior details: [0.0.6 release notes](docs/release-notes/v0.0.6.md).

---

# cpuv codebase review and improvement plan

Review date: 2026-11-10  
Reviewed version: 0.0.5  
Reviewed commit: `90f221fcbc6f187ba5a4e1c276428653ea577d86`  
Checkout: `C:\Users\Srihas\programming\project\cpuv`

This is an analysis-only report. No application source, configuration, tests, or dependencies were edited. The Desktop checkout supplied with the request no longer exists; the clean checkout above was located and reviewed instead. This report uses the filename `improvements.md` for the requested improvements document.

## 1. Overall assessment

cpuv already implements much more than a project scaffolder: compiler discovery, MSYS2 resolution, Conan/vcpkg adapters, archive verification, a content-addressed store, environment transactions, compiler shims, native builds, runtime bundles, offline transfers, generations, and cache maintenance.

The best next step is to make those existing workflows consistent and predictable before expanding the supported platforms or provider count. The most consequential gaps are preserving dependency intent during CLI edits, carrying declared tools/runtime settings through every build adapter, and making the distinction between a locked recipe graph and an exact binary installation explicit.

Existing strengths worth preserving:

- Dependency graphs remain provider-owned; cpuv does not guess library names from source includes.
- Shared-reader/exclusive-writer project leases and journaled commits already protect environment updates.
- Content verification covers installed files and defined compiler/SDK inputs.
- Downloads have checksum, transport, size, retry, and offline policies.
- Environment generations and quarantine preserve useful recovery evidence.
- Retained tests cover transactions, lock mismatch, archive handling, linking, quoting, repair, and application lifecycle.
- CI already includes formatting, Clippy, minimum-Rust checking, security audits, and separate native acceptance jobs.

### Evidence and priority labels

- **Source-confirmed:** the current control flow establishes the issue. A suggested end-to-end regression is not represented as already executed.
- **Risk / needs reproduction:** a specific suspicious boundary exists, but its operational consequence needs a controlled test.
- **Optimization:** a proposed performance improvement; no speedup is claimed without measurement.
- **Future feature:** an intentional capability addition, not evidence that the current implementation is broken.

**P1:** address before expanding scope; configuration loss or significant correctness/reproducibility impact.  
**P2:** fix in the next maintenance cycle; affected workflows fail or behave inconsistently.  
**P3:** maintenance, diagnostics, or lower-impact efficiency work.

Source references below use repository-relative paths and line numbers from the reviewed commit.

## 2. Review scope and verification

Reviewed the eight Rust workspace crates, concentrating on CLI dispatch, manifest/lock handling, provider integration, archive extraction, CAS publication, environment transactions, profile/build/run behavior, linking, toolchain identities, bundles, offline transfer, and cache ownership. Also inspected the Python source recipe, CI/release workflows, existing tests, README, and historical/current maintenance documentation. This was a targeted source review, not an exhaustive proof of every code path.

Executed:

```text
cargo test --workspace --locked --offline
```

Result: **77 passed, 0 failed, 5 ignored**, exit code 0.

| Suite | Passed | Ignored |
| --- | ---: | ---: |
| cpuv-cli unit tests | 21 | 0 |
| Isolated application lifecycle integration | 1 | 0 |
| Phase 1 / Phase 2 / Phase 3 integration entry points | 0 | 3 |
| cpuv-core | 22 | 1 |
| cpuv-fetch | 6 | 0 |
| cpuv-relocate | 12 | 0 |
| cpuv-shim library | 2 | 0 |
| cpuv-solver | 6 | 1 |
| cpuv-store | 4 | 0 |
| cpuv-toolchain | 3 | 0 |

The two other ignores are the crash subprocess helper, which is invoked by its parent regression test, and the manual provider-lookup benchmark. Doc-test targets contained no tests.

The test invocation built ignored Cargo output under `target/`; it did not change tracked implementation files. The lifecycle test operated on isolated temporary installation copies and a loopback release server.

Not executed in this review: native Phase 1–3 gates, the complete external-library matrix, new fault-injection cases for the findings below, release builds, a current dependency advisory scan, or performance benchmarks. Existing CI definitions and historical passing reports are useful evidence of infrastructure, but are not new passing results from this review.

## 3. Current bugs and behavioral inconsistencies

### B01 — Adding an existing dependency discards its detailed declaration

**P1 · Source-confirmed**

**Location:** `crates/cpuv-cli/src/main.rs:709–758`, `sync_request`.

The add path replaces an existing map entry with either a new `Dependency::Simple` or a new detailed reference whose options, linkage, and source are empty. It does not merge with the existing declaration.

**Trigger:** start with a dependency carrying `reference`, `linkage`, `options`, or a declarative source, then run `cpuv add NAME` or update its version through `add` without `--needed`.

**Impact:** a successful resolution can commit a manifest that silently drops build options, changes linkage, or changes a private/local source dependency into a catalog dependency. If resolution fails, transactional preservation still protects the old project; the issue is the successful replacement behavior.

**Suggested fix:** distinguish insertion from editing. For an existing detailed dependency, change only the requested version/reference field and preserve other fields. Reject incompatible source/reference transitions with an actionable diagnostic or require an explicit replacement operation. A bare repeated add should preserve the declaration.

**Regression:** add an already-declared dependency with static linkage and custom options; repeat with a private reference and a local source. Assert preservation of untouched fields and no unexpected provider transition. Also verify that a failed resolution preserves the complete prior manifest, lock, and environment.

### B02 — CMake project builds omit declared specialist tools

**P1 · Source-confirmed**

**Location:** `crates/cpuv-cli/src/build.rs:417–435`; `crates/cpuv-cli/src/source/tools.rs:63–71`. Compare `build.rs:56–75` and `source.rs:950–1043`.

The CMake project path prepares only CMake and Ninja, then builds PATH from those tools, the environment, compiler, and inherited PATH. It does not prepare the remaining `manifest.tools` entries or expose their executable directories. Other adapter/provider paths iterate the full declaration.

**Trigger:** a CMake project declares a pinned code generator in `[tools]` and uses `find_program` or a custom command to invoke it.

**Impact:** builds fail on a clean machine, or accidentally use an unrelated executable on the ambient PATH. Having downloaded the tool during dependency resolution does not make it discoverable in the later project build.

**Suggested fix:** share a declared-tool preparation result across adapters: verified executable paths, interpreter launchers, version checks, and ordered PATH entries. Feed it to configure, build, and test commands. Preserve the existing special handling that requires native CMake/Ninja executables.

**Regression:** use a pinned fixture generator absent from system PATH, including an interpreter-backed variant. Require a clean CMake build and offline rebuild to use that exact generator; place a deliberately different same-named executable on PATH to test precedence.

### B03 — Declared runtime search paths are omitted from native test commands

**P2 · Source-confirmed**

**Location:** `crates/cpuv-cli/src/build.rs:70–89, 196, 549–562`; compare `crates/cpuv-cli/src/runner.rs:300–313` and `runtime.rs`.

The direct runner appends `[runtime].search_paths` to PATH. Native build/test adapters construct another PATH without those directories. Calling `runtime.variables` adds declared variables and expands placeholders; it does not add search paths itself.

**Trigger:** a test executable or dynamically loaded plugin requires a DLL available only in a declared runtime search directory.

**Impact:** `cpuv run` can work while `cpuv test` fails on the same project. CTest, Meson, Make test targets, and MSBuild test invocations have inconsistent runtime behavior.

**Suggested fix:** one runtime-environment builder should resolve paths against the original project root, combine the compiler/provider/runtime directories in a defined order, and then apply explicit runtime variables. Use it for every execution/test adapter. Explicit PATH overrides need a documented policy.

**Regression:** put a test dependency only in `runtime.search_paths`; verify direct run and each supported native test adapter on a minimal PATH, including a named profile.

### B04 — CMake standard translation loses manifest semantics

**P2 · Source-confirmed**

**Location:** `crates/cpuv-cli/src/main.rs:384–392`; `crates/cpuv-cli/src/build.rs:489–497`.

The manifest accepts `c++03` and `gnu++03`, but the CMake adapter strips the prefix and passes `CMAKE_CXX_STANDARD=03`. CMake's supported standard values include 98, 11, 14, 17, 20, 23, and 26, not 03. The same translation also makes `c++20` and `gnu++20` indistinguishable because it does not set the extensions policy.

A related stale-cache case exists: changing a previously explicit standard to `default` simply omits the option on the next configure. It does not remove the earlier CPUV-injected `CMAKE_CXX_STANDARD` cache entry.

**Impact:** valid manifests can fail to configure, select GNU extensions unexpectedly, or retain an old standard after a user explicitly returns to compiler defaults.

**Suggested fix:** use an explicit standard mapping, including C++03 to CMake's 98 category where appropriate; set the extension policy deliberately; decide whether an unsupported requested standard must fail rather than decay. Track CPUV-owned cache settings and unset/reinitialize them when returning to `default`. Check the selected CMake/compiler capabilities before promising a standard.

**Regression:** cover c++03, gnu++03, c++20, gnu++20, an unsupported standard, and explicit-to-default transitions in a reused build directory. Inspect the actual compiler command.

**Reference:** [CMake CXX_STANDARD](https://cmake.org/cmake/help/latest/prop_tgt/CXX_STANDARD.html), [CMake CXX_EXTENSIONS](https://cmake.org/cmake/help/latest/prop_tgt/CXX_EXTENSIONS.html).

### B05 — Some build settings can bypass profile ownership checks

**P1 · Source-confirmed validation gap; resulting ABI failure needs a native regression**

**Location:** `crates/cpuv-cli/src/source/native.rs:278–334`; `crates/cpuv-cli/src/build.rs:223–254, 349–371, 498–511`.

Structured variables reject several compiler/profile-owned keys. However:

- The reserved list does not cover `CMAKE_BUILD_TYPE`, `CMAKE_CXX_STANDARD`, or runtime-affecting CMake settings such as `CMAKE_MSVC_RUNTIME_LIBRARY`.
- CMake arguments are appended after generated profile options.
- Make/MSBuild raw arguments are largely checked only for NUL bytes. They are appended after generated compiler/toolset settings.

**Trigger:** set `build.variables.CMAKE_BUILD_TYPE = "Debug"` in a Release dependency profile, or supply `CXX=...` / `/p:CLToolExe=...` through a raw argument channel.

**Impact:** the application can build with settings that disagree with the selected dependency profile while CPUV's saved metadata still describes the original compiler/configuration. These are trusted user inputs; this is a correctness gap, not a claimed privilege escalation.

**Suggested fix:** normalize effective adapter arguments before launching a build. Define which fields CPUV owns and validate all supported spellings/channels. Reject conflicting overrides with guidance to create a matching profile. If deliberate overrides are supported, include their effective values in the build identity and compatibility validation.

**Regression:** try equivalent overrides through structured variables, joined/split CMake arguments, Make arguments, MSBuild properties, and test arguments. Require either consistent rejection or a correctly fingerprinted new profile.

### B06 — Generic cpuv run commands lack the selected developer environment

**P2 · Source-confirmed**

**Location:** `crates/cpuv-cli/src/runner.rs:528–550`; compare the compiler branch at `runner.rs:576–583`.

The generic-command branch sets PATH, CPUV_ENV, and runtime variables, but does not apply `metadata.process_environment`. The managed compiler branch does.

**Trigger:** from a normal Windows terminal, select an MSVC profile and run a non-C++-driver tool through `cpuv run`, such as a helper that directly invokes the linker/resource compiler or reads INCLUDE/LIB.

**Impact:** selected tool executables can be found on PATH while their SDK/include/library variables are missing or inherited from the wrong developer environment.

**Suggested fix:** apply the verified selected toolchain environment to generic child commands, then apply the constructed PATH and explicit runtime overrides in a documented order. Do not mutate the parent shell.

**Regression:** launch from a clean environment with deliberately wrong ambient INCLUDE/LIB values; verify that a generic child sees the selected profile's values and that the parent environment stays unchanged.

### B07 — Profile creation overwrites an explicit provider choice

**P2 · Source-confirmed**

**Location:** `crates/cpuv-cli/src/profiles.rs:90–105`; `main.rs:511–534`.

Profile creation initializes a staged project using CLI choices, then overwrites `manifest.backend` and `manifest.vcpkg` with the original project's values.

**Trigger:** create a profile with `--provider conan` from a vcpkg project, or with `--provider vcpkg` from another provider.

**Impact:** the requested provider can be silently replaced by the parent provider. A profile intended for another compiler/provider can retain incompatible settings.

**Suggested fix:** make precedence explicit: CLI choice, then compatible copied project settings, then compiler-derived defaults. Copy vcpkg-specific settings only when the resulting provider is vcpkg, and validate the final merged profile before publication.

**Regression:** test auto/Conan/vcpkg parent-to-child combinations with and without explicit provider choices, including a compiler incompatible with the parent's provider.

### B08 — Raw compiler shim bypasses the bounded binary reader

**P2 · Source-confirmed resource-handling inconsistency**

**Location:** `crates/cpuv-shim/src/lib.rs:104–165`; compare `crates/cpuv-core/src/binary.rs` and `cpuv-relocate/src/inspect.rs`.

`deploy_dlls` reads each entire PE file into a Vec and compares existing/source DLLs by reading both fully. It does not use the bounded, mapped `binary::Data` reader.

**Trigger:** compile/link a large PE through the raw shim, particularly with large runtime DLLs or an existing DLL destination.

**Impact:** this path can allocate multiple binary-sized buffers and ignores the binary-inspection size limit used elsewhere. It also rescans DLL directories on fallback lookup.

**Suggested fix:** use the shared bounded PE reader for inspection and streaming digests or bounded streaming comparisons for equality. Index candidate DLL names once per deployment. Keep open-handle lifetimes compatible with later file copying/publication.

**Regression:** with a deliberately small CPUV_MAX_BINARY_MIB, verify consistent rejection in raw-shim and bundle paths. Measure peak memory with large fixture binaries; ordinary dependency copying must still work.

### B09 — Semantically identical locks can miss offline artifact caches

**P2 · Source-confirmed**

**Location:** `crates/cpuv-cli/src/artifacts.rs:4–15`; compare `crates/cpuv-core/src/locks.rs:65–70`.

Installed lock identity canonicalizes parsed TOML, but artifact cache identity hashes the raw provider payload bytes. Formatting-only edits to the provider payload therefore change artifact location despite preserving the installed resolution identity.

**Trigger:** preserve the same parsed provider lock but change its whitespace/key ordering, then restore with the active environment missing and only the previously saved snapshot available. For an envelope lock, its payload receipt must remain valid; changing envelope formatting alone does not cause this issue.

**Impact:** a semantically unchanged resolution can appear unavailable offline. Healthy active environments can mask the issue by regenerating a snapshot under the new key.

**Suggested fix:** base artifact identity on the same canonical provider-lock identity used for installed validation. Version the cache-key scheme and support verified lookup/migration of legacy snapshots. Keep exact payload bytes separately for diagnostics.

**Regression:** create a snapshot, change only provider-payload formatting, remove the disposable active environment, and require offline restoration from the old snapshot without network access.

### B10 — Make/MSBuild source copying rewrites unchanged inputs

**P2 · Source-confirmed redundant I/O; timestamp/rebuild effect should be measured**

**Location:** `crates/cpuv-cli/src/source/native.rs:192–209, 217–267`; `crates/cpuv-cli/src/build.rs:205–207`.

The reconciliation code computes current and previous digests and handles deleted-file ownership, but always calls `copy_snapshot` for every current file. That function unconditionally performs `fs::copy` and hashes the destination again.

**Impact:** a no-op build reads and rewrites all project inputs. Depending on filesystem copy timestamps and build-tool rules, it can also invalidate incremental builds. Large source/data trees amplify the cost.

**Suggested fix:** compare current, previous, and destination content. Skip copying unchanged, intact files; copy only changed/missing inputs. Preserve collision and deletion protections. Decide explicitly how to handle a build modifying a still-present copied input.

**Regression:** run two builds without changes and assert zero source-copy bytes on the second run. Check timestamps and compiler invocation counts. Modify one source, delete another, and modify a generated output; verify that only owned changes are reconciled.

## 4. Risks and limitations needing targeted reproduction

### R01 — Conan lock validation does not pin the exact binary/build-tool graph

**P1 design priority · Confirmed schema limitation; drift scenario not executed**

**Location:** `crates/cpuv-cli/src/source.rs:206–217, 738–746, 1277–1293, 1439–1465`.

The catalog lock stores dependencies, flags/configuration, compiler version, coarse ABI, a name-to-version map, and the upstream Conan lock. The post-install graph comparison checks host package name/version values, dropping recipe revision suffixes in that map and not retaining package IDs/package revisions there. The lock does not store the current portable compiler/SDK content digest or a full selected build-tool identity.

Existing protections matter: Conan's upstream lock pins recipe requirements/revisions, CPUV's provider cache key incorporates compiler content, CPUV contributes its profile key to Conan package IDs, and installed/snapshot contents are verified. This is not a claim that Conan recipe locking is absent.

**Remaining risk:** a locked sync may produce a different binary installation after a toolchain-content change or a different package revision while retaining the same high-level lock identity. Whether that is acceptable must be explicit.

**Suggested improvement:** distinguish recipe resolution, build-input identity, and deployed artifact identity. Record host/build contexts, recipe revision, package ID/revision, compiler/SDK digest, and relevant tools. Provide a strict mode that rejects drift and a clearly separate rebuild/migration operation.

**Regression:** keep recipe versions fixed while changing compiler contents or rebuilding a package revision. Check strict sync behavior and verify that graph diagnostics explain which identity changed.

**Reference:** [Conan lockfile semantics](https://docs.conan.io/2/tutorial/versioning/lockfiles.html).

### R02 — vcpkg search and installation do not share bootstrap serialization

**P2 · Source-supported race risk**

**Location:** `crates/cpuv-cli/src/vcpkg.rs:76–138, 325–336, 582–590`.

Sync acquires `vcpkg/operation.lock` before `prepare`; search calls the same mutating preparation function without that lock. Two searches or a search plus sync can initialize/bootstrap the same checkout concurrently. The outer shared cache lease permits such concurrent commands.

**Potential consequence:** publication failures or an executable/receipt observed between bootstrap steps. No concurrent failure was reproduced here.

**Suggested fix:** put a per-revision bootstrap lock inside `prepare`, so every caller participates. Publish the executable and verification receipt as a recoverable unit. Keep provider installation serialization separate.

**Regression:** use barriers around checkout publication, executable creation, and receipt creation; run search and sync concurrently against an empty disposable cache.

### R03 — Offline import publishes shared cache objects file by file

**P2 · Source-supported publication risk**

**Location:** `crates/cpuv-cli/src/offline.rs:230–254`; compare object publication in `source/tools.rs` and `artifacts.rs`.

Import validates collisions and copies individual files with `persist_noclobber`, but does not acquire the normal per-object tool/source publication locks. It holds the shared cache lease, so other import/bootstrap operations may run concurrently.

**Potential consequence:** another command can see an incomplete object, or simultaneous identical imports can fail on a benign file collision. Existing verification should prevent silent acceptance of incomplete content, but can turn the race into unnecessary failure/quarantine.

**Suggested fix:** group transfer files into known object types, stage and verify each object, then publish under the same object lock used by normal installation. Treat an already-published identical object as success.

**Regression:** interrupt an import after the receipt or an executable is copied; run a reader and a second identical import concurrently. Require complete old/new objects and recoverable staging, without mixed publication.

### R04 — Trust and platform support boundaries remain intentional

**Future hardening, not newly discovered vulnerabilities**

- Recipe/build execution uses the user's privileges. Provider offline flags are not an OS network sandbox.
- Local digest receipts detect content changes; they do not authenticate an author who can replace both data and receipt.
- Release checksums are downloaded from the same release location as executables. The updater does not independently verify the attestations produced by release CI.
- The current shipping workflow is Windows-oriented. Linux/macOS support requires platform abstractions and dedicated acceptance tests.
- ABI fingerprints and binary inspection are useful filters, not complete compatibility proofs.

Preserve these distinctions in documentation. Expand provenance verification, isolation, or platform support only with a defined threat/support model and corresponding tests.

## 5. Optimization opportunities

These are source-based opportunities, not measured end-to-end speedups.

| ID | Opportunity and evidence | Suggested approach | Measurement / correctness gate |
| --- | --- | --- | --- |
| O01 | Repeated SDK/environment hashing in `ToolIdentity::verify`, runner/shim entry points, artifact publication, and generation copies | Reuse verified inventories within a bounded operation; remove repeated passes over identical inputs where existing locking/handle guarantees permit. Design a persistent cache only after defining reliable invalidation. | Separate walk/hash/copy time; preserve mutation, added-file, and same-size-content-change detection. |
| O02 | Make/MSBuild always copy all source inputs; B10 | Incremental copy using the existing ownership receipt, current digest, and destination validation | No-op copied bytes and compiler process count; one-file-edit latency. |
| O03 | MSYS2 downloads complete before extraction begins; `main.rs` waits for `download_all` then extracts with concurrency 4 | Pipeline verified download completion into bounded extraction workers with backpressure | Cold install wall time, disk utilization, peak memory, and cancellation cleanup; never extract an unverified archive. |
| O04 | Source/bootstrap archive cache hashing and file writes are synchronous inside async paths; `source/archive.rs:38–80` | Move expensive hashing/extraction to bounded blocking workers; use async writes or a bounded writer pipeline. Reuse HTTP clients where policies match. | Concurrent tool-prepare latency and runtime responsiveness; identical checksum/publication behavior. |
| O05 | Conan sync checks the entire profile home twice with `cache check-integrity *:*`; `source.rs:1332–1335, 1425–1428` | Explore validating the resolved closure and known changed objects, retaining an explicit full-cache verification command | Small-project sync time as unrelated cached package count grows; corrupt a required recipe/package and require rejection. |
| O06 | pkg-config recompiles its Requires regex for each module and repeatedly scans the discovery list for the next zero-indegree node; `linking.rs:130, 364` | Cache the regex with OnceLock; use a deterministic ready queue for topological ordering; use membership sets alongside ordered output vectors | Synthetic chains/diamonds/cycles up to the current limit; identical link order and scoped flags. |
| O07 | MSYS2 conflict validation scans all selected packages for every conflict; repeated availability checks scan virtual providers; `cpuv-solver/src/lib.rs` | Extend the existing per-resolution provider index to conflict and availability lookups | Equivalent selected graph/error semantics across real names, virtual versions, ambiguous providers, and cycles. |
| O08 | Environment generations and artifact snapshots make independent full copies and repeated verification passes; `cpuv-core/src/generations.rs`, `artifacts.rs` | Measure duplication first; consider immutable blob manifests or supported copy-on-write cloning with a verified copy fallback | Physical allocated bytes, restore time, and isolation after active-file mutation. Do not hardlink mutable generations to active files. |
| O09 | A Tokio runtime is created before CLI parsing, including help/version; `main.rs:1266–1282` | Parse and dispatch synchronous commands before creating the async runtime | Repeated help/version/info startup measurements with the same release binary and antivirus state. |
| O10 | Raw-shim DLL reads/scans allocate and repeat work; B08 | Shared bounded PE reader, streaming equality, one case-insensitive DLL lookup index | Peak memory and DLL-deployment time; unchanged conflict detection. |
| O11 | Cache inspection/pruning hashes candidate objects, walks sizes, and takes an exclusive cache lease | Separate observational reporting from mutation; cache trustworthy size summaries with explicit accounting semantics; retain verification before deletion | Cache-inspection duration and command contention with many projects/generations; no deletion from stale ownership data. |
| O12 | Automatic full-graph re-resolution can cause unnecessary source work on small dependency edits | Preserve compatible existing resolutions where the provider permits; expose targeted upgrades separately | Number of changed packages and build actions after adding/removing one dependency; valid upstream constraint resolution. |

### Benchmark plan

Use a repeatable corpus with a tiny executable, a header-heavy library, a compiled shared-library graph, a multi-target native project, and a project with a declared code generator.

Record these separately:

1. First initialization and toolchain discovery.
2. Cold download/install with a controlled network fixture.
3. Warm locked sync and offline locked sync.
4. No-op build, one-source edit, one-header edit, and dependency edit.
5. Raw-shim compile overhead versus the real compiler.
6. Bundle/export/import and generation restore.
7. Cache inspection/prune with increasing project and artifact counts.

For each scenario record exact commit, release/debug mode, compiler/SDK identity, input size/file count, cache state, antivirus conditions, and hardware. Collect median and p95 wall time, peak working set, bytes read/written/downloaded, compiler subprocess count, and stage-level timings. The existing MSYS2 last-sync report is a useful starting point; Conan/vcpkg need similarly detailed timings.

Do not weaken checksum, compatibility, or publication checks to achieve a faster headline number. Any verification cache needs adversarial invalidation tests.

## 6. Maintainability and architecture improvements

### A01 — Centralize effective configuration

**Priority: P1; supports B01–B07.**

Define one typed effective configuration built from defaults, project manifest, profile, CLI overrides, and supported environment variables. Validate it before network or provider mutations. Record the effective provider, compiler, standard, configuration, flags, tool selection, and runtime plan.

This avoids independently merging configuration in profile creation, main CLI dispatch, Conan, CMake, and the runner. Keep explicit provider differences rather than pretending all providers have identical version/linkage semantics.

### A02 — Share build-tool and child-environment preparation

**Priority: P1; supports B02, B03, B06.**

A reusable build context should carry verified tool locations, interpreter launchers, source/build roots, compiler environment, runtime environment, and resource policy. Adapters should translate that context into provider-native arguments.

Keep compile-time and run-time environments distinct where needed, but derive both from the same declared inputs. Test adapter parity with small fixtures.

### A03 — Separate orchestration from the CLI

**Priority: P2.**

`main.rs` and `source.rs` each exceed 1,600 lines. Manifest schemas, dispatch, provider selection, tool preparation, and installation orchestration are closely coupled, with many modules importing parent symbols through `use super::*`.

Move schema/configuration and project operations behind a small internal library interface. Keep Clap parsing and user output at the edge. Use typed provider outcomes for the resolved graph, prepared environment, diagnostics, and publication plan.

Do this incrementally around the behavioral fixes; avoid a broad rewrite before regression coverage exists.

### A04 — Make identity and schema ownership explicit

**Priority: P1/P2; supports B09 and R01.**

Document separate versioned identities for:

- Dependency request.
- Provider resolution.
- Compiler/SDK and build-tool inputs.
- Built artifact.
- Installed environment.
- Source snapshot and transferable project.

Use one canonical serializer per identity. Keep original payload text and diagnostics as data, not competing identity definitions. Add schema migration tests and clear recovery instructions.

### A05 — Define one lock hierarchy and publication contract

**Priority: P2; supports R02/R03.**

Document ordering and scope for cache leases, project leases, provider-home locks, object locks, and build-output locks. Place ownership-sensitive locking in the primitive that mutates the object, so read-oriented callers cannot accidentally skip it.

Retain the existing journaled project commit. Extend the same complete-object publication principle to bootstrap/import boundaries. Add contention diagnostics and deliberate retry behavior instead of unexplained lock failures.

### A06 — Improve error categories and observability

**Priority: P2.**

Provide stable categories for invalid manifest, lock drift, unavailable offline input, integrity failure, busy environment, unsupported compiler/provider, upstream build failure, and runtime dependency failure.

Add consistent structured output for automation, with human-readable defaults and a verbose mode. Surface the effective provider and fallback reason. Preserve useful logs while redacting credentials and avoiding command/environment dumps.

## 7. Future product improvements toward a uv-like C++ workflow

These are proposed features, not current bugs. Preserve the project's Windows scope until its current workflows are reliable.

### F01 — Clear init/new/run/build lifecycle

**Near term.**

Add `cpuv new` templates for an executable, library, and tests, with optional CMake scaffolding and a useful .gitignore. Keep `init` suitable for adopting an existing project.

Consider a target-oriented `cpuv run` that builds/runs a declared target without requiring users to name the compiler. Maintain the existing explicit compiler command mode for experimentation.

Acceptance: a new user can create a project, add a dependency, run an executable, and run tests without manually constructing compiler/linker commands.

### F02 — Explicit resolution and synchronization modes

**Near term.**

Make the contracts for resolve, sync, upgrade, rebuild, offline, and strict lock checking easy to discover. Add a dry-run dependency/build plan that explains downloads, source builds, provider changes, and lock differences before execution.

Consider a convenience mode that syncs before run, but preserve a deterministic strict mode for CI. Do not silently change the current frozen-sync contract just to copy another tool's command names.

uv automatically locks/syncs in common project workflows and exposes stricter controls; the useful lesson is clear command semantics, adapted to expensive native builds. [uv locking and syncing](https://docs.astral.sh/uv/concepts/projects/sync/).

### F03 — Dependency explanation and controlled upgrades

**Near term.**

Add dependency-tree/why commands, direct-versus-transitive views, available-version inspection, and targeted upgrade operations. Explain why auto selected Conan instead of MSYS2 and show exact provider names.

Keep package identities provider-qualified when needed. A cross-provider suggestion should never silently rename or substitute a dependency.

Acceptance: users can explain why a package is present, why a constraint failed, and what will change in an upgrade.

### F04 — Editor integration

**Near term.**

Make the selected profile's compilation database easy to locate or export for clangd and other editors. Generate optional CMake presets and debugger launch hints derived from the effective configuration.

Do not require editing generated package files or adding global compiler/include paths.

Acceptance: switching a profile updates the editor configuration without changing the global Windows environment.

### F05 — Multi-project workspaces

**Medium term.**

Support workspace members, local member dependencies, shared policy, and explicit package/target selection. Decide whether locks are workspace-wide or grouped by compatible provider/toolchain profiles.

C++ workspaces must account for incompatible ABIs and build configurations; a single universal binary graph is not always appropriate. uv's shared-workspace structure is a useful UX reference, not a direct binary model. [uv workspaces](https://docs.astral.sh/uv/concepts/projects/workspaces/).

### F06 — Portable, declarative profiles

**Medium term.**

Allow named development/release/sanitizer/LTO profiles to declare toolchain requirements, supported targets, and features without relying exclusively on a machine-local compiler path.

Resolve those requirements to a local verified compiler and record exact content identity separately. Include cross-compilation host/build/target distinctions and reject unsupported combinations clearly.

### F07 — Shared binary caches and stronger provenance

**Medium term; after R01/A04.**

Add remote artifact caches only after the artifact identity is sufficiently complete. Separate read/write credentials, verify content, namespace by toolchain/profile inputs, and define eviction/repair policies.

Extend the existing release SBOM/attestation workflow into verifiable package provenance. Consider authenticated release metadata or attestation verification before the updater executes downloaded candidates.

Acceptance: a cache miss triggers an explained rebuild; a mismatched artifact never becomes an apparently compatible cache hit.

### F08 — Storage controls with recoverability

**Medium term.**

Add explicit generation/quarantine retention policies, physical versus logical disk reporting, and an explainable project-forget operation. Keep recovery stores protected by default; a quota should report protected bytes and unmet targets.

Archive/CAS garbage collection needs reachability from active locks, generations, transfers, and interrupted transactions. Provider-owned databases should remain under provider-native maintenance unless a reliable ownership contract exists.

### F09 — Broader platform support

**Later milestone.**

Extract process management, compiler probing, executable inspection, runtime search, shell activation, and install/update behavior behind platform boundaries. Add Linux ELF and macOS Mach-O handling with native CI before advertising support.

Treat Windows ARM64 native execution, x64 emulation, and cross-compilation as distinct support claims. Publish the tested matrix by compiler, architecture, runtime, provider, and build system.

### F10 — Specialist C++ workflows

**Later / demand driven.**

Explore C++ modules, sanitizers, coverage, benchmarks, fuzz targets, package publishing, documentation targets, and optional IDE project generation. Each needs a defined compiler/build-system capability check and reproducible configuration.

Avoid implementing a new general-purpose build engine until there is evidence that established native build systems cannot meet the intended workflow.

## 8. Regression strategy

Prioritize small deterministic cases instead of relying only on large upstream-library matrices.

| Area | Required additions |
| --- | --- |
| Manifest editing | Repeated add, version-only edits of detailed dependencies, explicit source replacement, failed-resolution preservation |
| Adapter parity | Same runtime search paths, tool versions, interpreter handling, developer variables, and configuration semantics across adapters |
| Build state | c++03 mapping, strict/GNU extensions, explicit-to-default standard change, compiler/configuration change in an existing build directory |
| Identity | Canonical lock formatting, compiler content drift, Conan package revisions, build-tool identity changes |
| Concurrency | Simultaneous bootstrap/search/sync, identical offline imports, interrupted per-object publication |
| Resource bounds | Raw-shim large PE reads, decompression limits, timeout/cancellation, excessive metadata/response files |
| Incrementality | Zero rewritten source files on a no-op build, one-file changes, deleted inputs, generated-file collisions |
| Windows paths | Spaces, Unicode, case differences, long paths, reserved names, junction ancestors, cross-volume copy fallback |
| Solver/linking | Differential pacman/pkg-config fixtures, version edge cases, virtual providers, cycles, static-private dependencies, deterministic ordering |

Retain the current native release gates; ordinary unit-suite success does not replace them. Add adapter-specific fixtures to those gates and run targeted tests first.

Useful follow-up checks include fuzzing parsers (repository descriptions, lock envelopes, response files, pkg-config, PE/archive metadata) and generated dependency graphs. These are proposed checks; no fuzzing campaign was performed for this report.

## 9. Historical audit reconciliation

The older encyclopedia chapter is explicitly historical. Do not copy its findings into the current backlog without checking the implementation.

| Historical item | Current assessment |
| --- | --- |
| I1: nested catalog archives skipped | Addressed: archive inspection is recursive and a regression checks nested archive architecture. |
| I2: MSYS2 rollback drops staging backups | Addressed in the current shared journal/commit implementation; crash and failed-restore tests passed. |
| I3: metadata reused mainly by existence | Addressed: reuse verifies sealed contents, request/lock identity, and tool identity. |
| I4: refresh semantics differ | Current docs/help define refresh during resolution and preserve locked sync. This is a contract to keep testing, not automatically an unresolved bug. |
| I5: linkage label ignored | Addressed: Conan shared options are supplied and resulting linkage/artifacts are checked. |
| I6: environment mutation races with readers | Substantially addressed by reader leases and exclusive project mutation. Keep separate object/import races in R02/R03. |
| I7: no retained behavioral CI tests | Obsolete: checked-in unit/integration tests and native CI gates exist. |
| I8: uppercase DLL/.pc handling | The relevant catalog DLL/.pc comparisons are now case-insensitive. Continue testing other metadata formats separately. |
| I9: bounded provenance/offline isolation | Still a documented boundary; see R01/R04 and F07. |
| I10: no retention, duplicated transactions, weak init/report handling | Partly obsolete: prune/log commands, shared journaled transactions, transactional init, and warning-only post-commit report failures exist. Module coupling and long-term retention remain improvement areas. |

## 10. Recommended implementation order

1. **Configuration preservation and adapter correctness:** B01, B02, B03, B05, B06; introduce the shared effective configuration/build context as needed.
2. **Profile and build-state consistency:** B04, B07, B10, with native tests proving actual compiler/runtime behavior.
3. **Identity and publication reliability:** B09, R01, R02, R03; version migrations before expanding artifact sharing.
4. **Resource and warm-path performance:** B08 and benchmark-backed O01–O12. Preserve current integrity guarantees.
5. **Developer experience:** F01–F04, structured diagnostics, and targeted upgrade planning.
6. **Larger scope:** workspaces, remote caches, richer profiles, and new platforms only after the previous gates are stable.

A maintenance release should have no silently discarded dependency fields, consistent declared-tool/runtime behavior across adapters, explicit handling of profile overrides, and regression coverage for lock/artifact identity and concurrent publication.

All fixes and features above are recommendations. This review changes only this Markdown report.
