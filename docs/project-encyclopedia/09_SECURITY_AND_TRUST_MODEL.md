# 09 · Security and trust model

[Index](MASTER_INDEX.md) · [Issues](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md) · [Existing policy](../../SECURITY.md)

## Scope and identity

This is a source audit, not a penetration test, signature verification audit, or dependency advisory assessment. No CVE absence, exploitability, or overall security certification is claimed. The CLI has no account authentication, roles, sessions, network listener, or service-to-service authorization. It acts with the current user's Windows permissions. Files and PATH updates are protected by ordinary OS permissions, not an application identity boundary.

## Trust boundaries

1. User-controlled commands, environment variables, manifests, locks, and link flags enter the local process.
2. Remote repositories supply MSYS2 metadata, archive checksums, package contents, provider metadata, and application releases.
3. The shared writable cache is reused across projects and sometimes hardlinked into their environments.
4. GCC, Conan recipes, upstream build systems, explicitly requested pacman transactions, and compiled programs execute code with user permissions.
5. The native installer crosses into the user's executable directory and registry PATH. Self update executes a downloaded candidate before replacing the current executable.

Checksums detect content disagreement, not malicious publishers that control content and expected digests. No upstream package signature verification, Authenticode signing implementation, sandbox, hermetic environment, encrypted cache, or dedicated secrets manager was found.

## Implemented controls

`policy::allowed_url` accepts HTTPS or loopback HTTP identified through host/IP parsing. The archive client's redirect policy rejects disallowed destinations and ten or more previous redirects. Connection/read deadlines are 15/60 seconds; optional whole-transfer deadlines and separate archive/index/extraction bounds limit some resource use. These policies cover first-party HTTP clients, not arbitrary Conan recipe calls.

`parse_desc` validates SHA-256 shape, namespace, and archive filename. `safe_relative` rejects absolute/parent traversal components, drive/alternate-stream separators, trailing spaces/dots, and Windows device names. Package extraction limits entries/expanded bytes, validates namespace prefixes, resolves links lexically without creating OS symlinks, and rejects unresolved cycles/special files. Tool extraction rejects symbolic links and unsafe ZIP names. Catalog deployment checks containment and rejects observed symlinks.

CAS reuse checks metadata equality, file sizes, and BLAKE3 digests. Materialization rejects differing case-insensitive path collisions. Metadata relocation replaces files privately. Binary inspection checks supported PE/COFF conditions but explicitly reports that compatibility is not guaranteed. Argument vectors use `Command` rather than shell-concatenated compiler commands. PowerShell activation separately quotes generated paths. Explicit flags and upstream scripts remain trusted executable inputs.

## Findings and mitigations

| Finding | Severity / confidence | Impact and practical mitigation |
| --- | --- | --- |
| Unsigned upstream and release trust | High trust dependency; confirmed absent control | Compromised publishers can provide executable content with matching checksums. Use trusted catalogs/releases; add independent signatures/provenance as future work. |
| Conan recipes execute during resolution | High trust boundary; confirmed | Even prebuilt installs execute upstream Python. Review catalog trust and isolate untrusted builds using OS mechanisms outside cpuv. |
| Catalog archives skip a COFF inspection branch | Medium; confirmed source condition | `.a/.lib` inspection requires an environment-root `lib` prefix, while catalog packages are nested under `full_deploy`. Generalize inspection ownership before claiming catalog archive coverage. |
| MSYS2 rollback recovery can fail without explicit backup retention | High operational risk; source-supported potential failure | A second filesystem error during restoration may discard the temporary backup directory. Unify hardened commit logic and inject controlled filesystem failures in future tests. |
| Local generated metadata can bypass full reuse validation | Medium; confirmed check limitation | Reuse checks existence of local metadata, not its hashes. Keep generated files immutable; regenerate an environment and add inventory validation in future. |
| Arbitrary build/profile values may appear in provider logs | Medium; confirmed logging behavior | Do not put credentials into flags/URLs. Restrict log access and add explicit redaction where future integrations introduce secrets. |
| Build and project locks have different scopes | Medium; potential race | Installation and running programs can overlap. Avoid updates during running programs; design a shared environment-use lock if concurrent mutation becomes supported. |

These are audit findings, not fixes performed during this documentation task. Detailed affected symbols, reproduction conditions, and proposed regressions are in [chapter 19](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md).

## Non-findings and limits

No first-party password/token configuration was found. GitHub Actions uses `secrets.GITHUB_TOKEN` through `GH_TOKEN`; the actual token is neither inspected nor copied here. TLS relies on reqwest/rustls for Rust clients and platform PowerShell behavior for bootstrap; there is no certificate pinning. Repository code contains no dependency-audit workflow. Transitive source, compiled tool internals, upstream recipes, live release infrastructure, and Windows DLL loader search details were not exhaustively audited.

Sources: [HTTP policy](../../crates/cpuv-fetch/src/policy.rs), [extract/store](../../crates/cpuv-store/src/lib.rs), [tools extraction](../../crates/cpuv-cli/src/source/archive.rs), [catalog](../../crates/cpuv-cli/src/source.rs), [inspection](../../crates/cpuv-relocate/src/inspect.rs), [updater](../../crates/cpuv-cli/src/update.rs), [installer](../../crates/cpuv-cli/src/install.rs), [release workflow](../../.github/workflows/release.yml).
