# 0.0.5 maintenance review

This guide documents 0.0.5 and updates the October project encyclopedia. For the earlier release, see the [0.0.4 notes](release-notes/v0.0.4.md).

## Correctness and recovery

- Sealed environments record a canonical provider-lock identity. Verify, managed builds/runs, raw compiler shims, bundles and development exports reject a different or missing installed lock. Formatting and envelope changes preserve the identity. Saved compiler/linker/environment state is checked against a sealed copy before build adapters consume it. Empty, newly initialized projects can run without a lock. Older locked environments require sync to regenerate metadata; provider locks with older compiler-identity coverage can require an explicit lock migration. A cache without the necessary healthy inputs may require online restoration.
- The installed provider payload is retained inside the sealed environment. A generation created after a user edits the manifest or lock keeps the installed request/resolution, rather than pairing older libraries with newer declarations.
- Damaged immutable snapshots, malformed/missing tool receipts, changed tool payloads, corrupted archives and unusable online index snapshots are preserved in quarantine before reconstruction. Offline mode never substitutes unverified content; errors identify missing verified inputs. Publication uses per-object locks.
- Archive retries for the official MSYS2 UCRT64, MINGW64 and MINGW32 repositories can use the official mirror service. The lock retains its original repository, indexed length and SHA-256. A custom repository owns its own retry path.
- Compiled packages with ambiguous or missing automatic link metadata can be installed. The link plan records packages needing explicit selection. Projects can choose two independent modules and keep other packages installed for future consumers or native build targets.
- Application asset enumeration includes ordinary files named files.json, including uppercase/nested variants, and preserves empty directories. Environment receipt exclusion is a separate operation.
- Bundle outputs must be outside asset/dependency input directories, and development exports outside the physical source tree. Output checks resolve existing junction ancestors before creating staging directories, preventing recursive self-inclusion.
- Toolchain identities include the selected C driver, GCC private include roots, and discovered Microsoft resource tools. This remains a defined input closure rather than a promise to track every action an external recipe performs.
- Make/MSBuild source copies track copied-file ownership and remove deleted inputs. Generated outputs remain. A modified copied input that would be deleted causes an explicit error. Legacy copies without ownership receipts are preserved under source-backups and regenerated.
- MSBuild tests receive the declared runtime variables, as do the other adapters. The root placeholder refers to the original project source directory, including for copied Make/MSBuild projects.
- Short compiler/tool/updater probes have a 30-second deadline (Microsoft developer-environment setup has a separate 120-second allowance), a combined 2 MiB diagnostic cap, and whole-tree cleanup on Windows. Managed build deadlines remain separately configurable. Interactive run/shell behavior is preserved.
- Bootstrap overrides are selected before default tools are prepared. Make/MSBuild do not require otherwise unused CMake/Ninja downloads. Direct CMake/Ninja/Conan overrides must be native executables; specialist tools can declare interpreters.
- Info, profile listing and activation reads use project reader leases. Clean takes the environment/build locks. Interactive shells release their read lease before opening; each raw shim invocation still acquires its own lease.
- Working directories used only for toolchain/search/doctor operations are not automatically permanent cache roots. Import registers the actual destination before activation/provider restoration, retaining its references if a later step fails.
- Dependency recursion is explicitly bounded at depth 128. Excessively deep graphs fail with an error instead of consuming the process stack without a bound.

## Performance and resource handling

Cache maintenance builds the artifact request index once per operation, rather than reading every artifact once per active/historical environment. Size reports describe logical file bytes, including hardlinks, and report protected candidate bytes and missing registered projects.

An online MSYS2 add validates the staged environment and its runtime closure before publishing it. It leaves the verified archive and CAS content available for locked restoration. An offline sync or development export makes the independent deployed snapshot when needed. Reusing the same installed environment leaves the existing generation history in place.

Compiler hashes already checked within a publication operation can be reused when selecting its compiler-only identity. A final content verification still checks for changes before publication. Artifact restoration captures the compiler identity once and retains the boundary verification. There is no persistent timestamp-only hash shortcut.

Binary inspection uses a read-only file mapping on Windows with a handle that denies concurrent writes/deletion through hardlinks. It releases views before publishing bundles. This avoids an archive-sized heap allocation; parser tables and touched mapped pages still consume memory. CPUV_MAX_BINARY_MIB defaults to 4096 and accepts 1–1048576; files above the limit fail with an explicit message. Non-Windows library builds use a bounded read fallback. This is an input-size bound, not a universal process-memory guarantee.

Virtual package providers have a reverse index scoped to a resolution. Selection, version constraints and ambiguity errors retain their existing semantics. Blocking cache lock acquisition is moved to Tokio blocking workers in async download/bootstrap paths, including shared-archive downloads; lock waiters do not block the async task that must finish the corresponding transfer.

A five-sample debug synthetic benchmark on this workstation compared 500 virtual lookups in 3,000 packages: median scan lookup 84.396 ms, indexed lookup including index construction 5.676 ms, with equal selected results. This measures provider lookup only, not end-to-end installs. Run cargo test -p cpuv-solver provider_lookup_benchmark -- --ignored --nocapture to reproduce.

Treat integration wall times as observations. Cold bootstrap, warm reuse, antivirus, SDK size and network/toolset versions must be recorded before comparing performance. These changes do not claim a universal speedup or a universal compatibility guarantee.

## Storage ownership and retention

| Storage | Policy / owner |
| --- | --- |
| Immutable environment/tool/source snapshots | CPUV cache prune previews unused verified objects; apply requires the exclusive cache lease |
| Registered projects, generations and interrupted transactions | Protect referenced objects; missing registered projects cause conservative retention |
| Build/provider logs in recognized CPUV log directories | Explicit age-based preview/apply through cache logs |
| Quarantine, lock backups, generation/source backups | Retained for recovery and diagnosis; no automatic deletion by age |
| Download archives and CAS objects | Retained by this collector; a quota does not override protected ownership |
| Conan/vcpkg provider databases | Provider-owned; use provider maintenance |
| Project source and unrelated user files | Never cache-collection candidates |

    cpuv cache inspect
    cpuv cache prune --quota-mib 2048
    cpuv cache logs --older-than-days 30
    cpuv cache logs --older-than-days 30 --apply
    cpuv cache forget -- C:\path\to\intentionally-removed-project

Logs selects recognized CPUV .log names only, leaves recent/unrecognized files alone, and retains all recovery stores. Review previews before applying. A quota cannot promise to reach its target when retained/provider-owned storage dominates.

## Resolution and transfer contracts

| Operation | MSYS2 | Conan | vcpkg |
| --- | --- | --- | --- |
| sync | Restore the locked package graph | Restore the locked provider/profile graph | Restore/verify the pinned graph and installed status |
| --refresh sync | Preserve the lock; no graph update | Preserve the lock; no graph update | Preserve the lock; no graph update |
| lock / changed add or remove | Resolve against fresh metadata | Explicit resolution/update, subject to upstream options | Resolve the manifest at the explicitly pinned registry revision |
| --offline sync | Verified archives/CAS; no transfer | Verified deployed artifact or sufficient cached provider closure | Verified deployed artifact/intact installed state; rebuilding missing provider inputs is not implied |
| --from-source | Select the source provider | Rebuild according to upstream recipes and their prerequisites | Not a substitute for changing vcpkg's own recipe/triplet contract |

The refresh flag concerns metadata during resolution, not silently updating a frozen graph. Source-package names/options remain upstream-owned. Source and offline fixture tests use controlled local inputs to check these contracts.

Project-relative local inputs may point to a sibling directory. A development transfer carries cached source snapshots and deployed outputs; it does not copy arbitrary sibling directories into the destination. Missing original local inputs can use their verified cache snapshot; a present but changed local input is rejected. Rebuilding every recipe offline additionally requires its full provider source/tool closure.

An explicit redistributable compiler tree is optional. Licensed compiler/SDK prerequisites remain external unless redistribution has been explicitly declared and is supported. Compiler/SDK identity checks still apply after relocation. Graph diagnostics can contain original staging paths; installed compiler/link metadata and provider-generated CMake/pkg-config interfaces are the usable integration paths.

Import requires a new destination. If activation/provider restoration fails, preserve that destination and the transfer for diagnosis; imported references remain registered. A provider-cache restore failure is reported separately from successful restoration of the project/deployed environment. Restore the retained provider archive through the provider's native maintenance interface before dependency rebuilds that need it.

## Local verification

The [November 4 live matrix](project-matrix.md) records the 0.0.5 candidate checks and the package issues found during installation. The November 2 checkpoint below records the earlier maintenance fixes. Its source and executable hashes, logs, inventory and audit output are retained locally under `tmp/review-fixes-*`.

| Check | Result |
| --- | --- |
| Debug and optimized workspace suites | Each: 70 passed, 0 failed, 5 explicit/manual ignores |
| Formatting, Clippy with warnings denied, Rust 1.88 workspace/all-targets | Passed |
| Optimized GCC Phase 1 | Passed; lock/toolchain drift, corruption repair, bundle assets and recursive-output rejection |
| Optimized Phase 2 | Passed; source inputs, generations, native builds, fresh-cache Conan/MSYS2 transfers, offline repair and cache retention |
| Optimized MSVC Phase 3 | Passed; C/C++, library components, generated code/resources and provider-outage preservation |
| Native adapter and vcpkg scripts | Passed; Meson, MSBuild runtime variables, specialist tools, plugins/resources, missing-environment repair and minimal-PATH bundles |
| RustSec audit / dependency inventory | 0 reported vulnerabilities or warnings; 204 inventory components |

The five ordinary-suite ignores are the three explicitly invoked native gates, a subprocess crash helper and the manual benchmark. MSVC, vcpkg and adapter checks ran at implementation checkpoints before the final unrelated output-location/transfer-bootstrap corrections; the final workspace and GCC/transfer gates include those corrections. Clean VM, native ARM64 and unavailable historical-toolset coverage remain pending.

## Trust and release boundary

Builds execute trusted upstream/local recipes with the invoking user's permissions. Job Object limits control resource use and cancellation, not filesystem, registry, credential or network access. Offline/no-remote operation is not enforced network isolation.

The updater still uses HTTPS, release checksums and the downloaded executable's version. It does not enforce a pinned signing key or GitHub attestation. Build provenance remains a separate release-workflow artifact. Authenticode credential provisioning, upstream package-signature policy, mandatory attestation enforcement and automatic VM isolation require explicit trust/credential/infrastructure choices; they are not silently enabled by this maintenance change.

The GitHub release workflow runs the optimized Phase 1, Phase 2 and Phase 3 gates, inventories and audits the dependencies, checks the tag version and attests the built assets before publication. Clean desktop Windows 10/11 images, native ARM64, historical toolsets and additional GUI and library combinations have separate verification requirements. See [Phase 3](phase3.md) for lab setup and release boundaries.
