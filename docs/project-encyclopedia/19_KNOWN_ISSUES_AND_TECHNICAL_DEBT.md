# 19 · Known issues and technical debt

[Index](MASTER_INDEX.md) · [Status](16_IMPLEMENTATION_STATUS_AND_ROADMAP.md) · [Security](09_SECURITY_AND_TRUST_MODEL.md)

These findings were documented without broadening the requested code change. Severity reflects impact if the condition occurs, not a CVE score. Reproduction marked proposed was not executed against user data. Confirmed source limitations are distinguished from potential races and recommendations.

## I1 · Catalog static archives bypass COFF member inspection

**Confirmed; medium; high confidence.** [`inspect.rs::environment_with_runtime_policy`](../../crates/cpuv-relocate/src/inspect.rs) checks `.a/.lib` members only when the environment-relative path starts with `lib`. [`source::prepare_metadata`](../../crates/cpuv-cli/src/source.rs) deploys catalog archives under `full_deploy/host/NAME/VERSION/lib`, so they do not enter that branch. A retained compiled fmt/zlib environment was inspected: it contains four `.a` files while its report records `library_archives: 0` and `coff_members_checked: 0`.

Impact: catalog archive architecture coverage is incomplete, even though PE/DLL traversal and other guards exist. Workaround: inspect those archives independently and do not treat the report as a full compatibility proof. Recommended fix: inspect owned archives recursively independent of root layout. Proposed regression: deploy identical valid/mismatched archives under root lib and nested provider folders and require equivalent validation. No malformed binary was injected during this audit.

## I2 · MSYS2 rollback can lose recoverable backups after restoration failure

**Source-supported failure risk; high operational impact; high confidence in control-flow gap.** [`main.rs::sync`](../../crates/cpuv-cli/src/main.rs) restores swapped artifacts using fallible operations inside a temporary staging directory. A second error can propagate before restoration finishes; it lacks [`source::commit`](../../crates/cpuv-cli/src/source.rs)'s explicit `stage.keep()` recovery branch.

Impact: normal-error rollback is not a guaranteed preservation mechanism under repeated filesystem failures. This is not a claim that every ordinary error loses data. Workaround: stop processes holding environment files and preserve surviving stage/backup content before manual repair. Recommended fix: common hardened transaction code that retains backups on every restoration failure. Proposed regression: controlled file-operation fault injection at each swap/restore boundary. Not executed against live user projects.

## I3 · Local metadata reuse is checked mainly for existence

**Confirmed check limitation; medium; high confidence.** MSYS2 `environment_reused` compares install identity, requires metadata files to exist, and checks payload paths; it validates the CAS payload separately. It does not hash the local generated manifest/link plan/specs/report before reuse. Privately relocated metadata can differ without changing the cache hashes.

Impact: edited generated metadata can survive reuse or fail later rather than being repaired automatically. This is not a separate-user privilege boundary. Workaround: keep generated files immutable and regenerate the environment using preserved portable config. Recommendation: verify generated metadata inventory or regenerate metadata deterministically on reuse. Proposed regression: alter a disposable environment's private metadata and confirm detection/repair.

## I4 · Catalog `--refresh` differs from MSYS2 refresh

**Confirmed source behavior; low to medium; high confidence.** The MSYS2 index path reads `cli.refresh`. Calls to `source::sync` pass `force_resolve` as the refresh parameter; the catalog adapter does not directly read `cli.refresh`. A matching catalog lock can therefore be reused under a global `--refresh` flag.

Workaround: `cpuv lock` intentionally requests catalog re-resolution/update; it is a graph-changing operation. Recommended fix: explicitly define separate index/revision update semantics and align flags/help. Proposed regression: use a controlled provider revision change and check sync, refresh and lock independently. No live repository revision mutation was performed.

## I5 · Detailed linkage label does not force catalog shared options

**Confirmed interface limitation; medium; high confidence.** `Dependency::version` accepts optional `linkage = "shared"`; catalog installation does not translate that declaration into a Conan option. Its actual libraries follow upstream package defaults/metadata, which can be static. The public/private interface parser also does not implement a general static-private model.

Workaround: use documented default/provider behavior and explicit supported link interfaces, without assuming this label changes builds. Recommended fix: either reject unsupported forcing semantics or implement options, identities, locks, runtime deployment, and validated link plans together. Proposed regression: request declared linkage and assert graph options/artifacts rather than only successful parsing.

## I6 · Build and project mutation locks are separate

**Potential concurrency race; medium; medium confidence about runtime consequences.** `runner::run` holds build.lock through the managed program; package updates hold project.lock, not the same lock. Raw shim invocation has another lifecycle. A package environment may be renamed/replaced while another command has loaded its metadata or is using DLLs.

Impact depends on Windows file locks and timing: errors, mixed observations, or update refusal are possible. Workaround: avoid dependency changes while programs/builds use the environment. Recommendation: define shared environment readers and exclusive mutation locking. Proposed regression: deterministic synchronization barriers between metadata read, package swap and DLL use. No race exploit was attempted.

## I7 · No retained behavioral CI suite

**Confirmed quality debt; medium; high confidence.** The source distribution excludes tests; CI only formats/lints/builds. External six-scenario removal validation passed, but future contributors cannot run a checked-in equivalent from this tree alone.

Recommendation: establish a maintained reproducible suite in an explicitly approved distribution/location, prioritizing transaction, archive, solver and compiler boundaries. Workaround: preserve exact external validation procedures and results without representing them as repository tests. Proposed regression infrastructure is a future recommendation, not an existing workflow.

## I8 · Catalog extension matching and diagnostics have portability edges

**Source condition, not a demonstrated upstream failure; low to medium.** `prepare_metadata` tests DLL and `.pc` extensions using case-sensitive string equality, while Windows names and binary inspection use different normalization rules. Uppercase DLL extensions may not be centralized into env/bin. Generated graph JSON is also stored with original staging paths; only selected metadata formats are relocated.

Recommendation: normalize recognized extensions and distinguish graph diagnostics from usable deployment paths. Proposed regression: disposable provider output with uppercase extensions and relocated stage references. Workaround: inspect actual deployment/search paths; do not rely on generated provider batch files as an advertised public cpuv integration interface.

## I9 · Profile/provenance and offline isolation remain bounded

**Documented limitation; potentially high trust impact.** Compiler/profile hashes do not capture every support file, environment input, upstream option, or executable action. `--no-remote` controls Conan remotes, not arbitrary subprocess networking. Upstream recipes can reduce package identities intentionally. Source locations: `source::sync`, `configure_profile`, and external provider integration.

Recommendation: state support boundaries, capture richer provenance where required, and use OS isolation for untrusted builds. Proposed regression: controlled environmental inputs and attempted subprocess network access under a defined offline policy. No claim of sandboxing should be added without that design.

## I10 · Cache, logging, and transaction code maintenance

**Confirmed maintenance boundaries; low to medium.** Caches/logs have no automatic retention/GC; installs can accumulate compiler/profile variants. Binary/catalog commit logic is duplicated. Source module naming is historical and the adapter imports parent symbols through `use super::*`. Init and post-commit diagnostic writes have weaker atomicity than the central catalog commit.

Recommendation: define retention/repair requirements and centralize transaction/schema ownership before expanding provider count. Workaround: preserve caches intentionally for offline recovery and manage disk use explicitly. Proposed regressions should cover successful post-commit state even when report writing fails, as well as repeated profile changes and retained recovery data.

## Explicit non-claims

No dependency advisory scan, penetration test, live GitHub infrastructure audit, exhaustive upstream recipe inspection, minimum-platform matrix, or private historical benchmark recovery was performed. These remain UNVERIFIED, not confirmed vulnerabilities or passing assurances. No claim that a stub exists follows merely from a deferred feature; the source scan found no `todo!`/`unimplemented!` production placeholders.
