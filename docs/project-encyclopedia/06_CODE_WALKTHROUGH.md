# 06 · Execution walkthrough

[Index](MASTER_INDEX.md) · [Architecture](03_ARCHITECTURE_DEEP_DIVE.md) · [Interfaces](08_API_AND_COMMUNICATION_PROTOCOLS.md)

## Contents

- [Startup and initialization](#startup-and-initialization)
- [MSYS2 synchronization](#msys2-synchronization)
- [Catalog synchronization](#catalog-synchronization)
- [Removal](#removal)
- [Compilation and execution](#compilation-and-execution)
- [Installation and updating](#installation-and-updating)

## Startup and initialization

Trigger → `main.rs::main` → installer-name dispatch, compiler-name dispatch, or Tokio runtime → `Cli::parse` → `execute`. Toolchain, doctor, and self-management commands take early paths. Other commands canonicalize `--project` (default current directory). The ordinary CLI does not search every parent folder for a manifest; pass `-C` when needed. The raw shim's environment lookup does inspect ancestors.

`init` acquires `.cpuv/project.lock`, rejects an existing manifest, and probes GCC. Discovery precedence is explicit compiler, CPUV_CXX, CXX, PATH, then bounded MSYS2 discovery. `probe_with_flags` queries target, preprocessing macros, verbose configuration, private runtime headers with a fallback, accepted language modes, and GCC's reported linker. `init` selects an initial backend/standard, installs the shim, creates include/lib/bin directories, writes runtime metadata and activation, then saves `cpuv.toml`. This is a sequence of writes, not the full staged synchronization transaction; initialization failures can leave partial `.cpuv` state.

## MSYS2 synchronization

`add` separates the explicit `base-devel` host request from library requests. `--needed` attempts to keep an already satisfied compatible lock. `sync` acquires the project lock, reads/validates the manifest, probes the compiler, and edits a local manifest copy for add/remove requests. It routes to catalog mode for explicit catalog/source flags, a persisted catalog backend, custom profile flags, a changed compiler, no eligible binary provider, or a profile reset.

Otherwise a no-update sync replays the version-1 lock after checking ABI, repository, roots, and graph reachability. A resolve operation fetches the snapshot and checks missing root names for automatic catalog fallback. `resolve_for` recursively visits dependencies, validating constraints even when a package was already selected, tolerates graph cycles through a visiting set, and checks selected conflicts. A virtual provider lookup scans candidates and rejects ambiguity. It is not a backtracking historical version solver.

For selected runtime packages, a differing compiler major routes to catalog mode. `Store::cached` verifies metadata, path safety, file size, and BLAKE3 content. `Fetcher::download_all` limits unfinished network futures; each new archive streams into a temporary file, counts bytes against indexed length, hashes SHA-256, and publishes only after verification. Ingestion uses a separate four-job blocking pipeline. Prefix-aware tar extraction validates paths, materializes safe links without creating OS symlinks, discards recognized host metadata, and publishes package content records.

Materialization deduplicates case-folded paths and rejects differing content at one path. `index_environment` relocates `.pc` and selected `.cmake` content. `configure_environment` builds ownership/interface metadata, inspects binaries, checks ABI-related definitions/options, and writes compiler metadata/specs. Commit swaps the prepared artifacts; local reports follow. See [storage](07_DATA_MODEL_AND_STORAGE.md) for schemas and [issues](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md) for reuse/recovery limitations.

## Catalog synchronization

`source::sync` reads the old lock's schema/profile, accepts versions 1–3 for transition, fully parses version 3, and merges saved flags with current environment values. `--reset-profile` discards saved values first. It re-probes and executes a compiler/linker/runtime doctor check before any provider installation.

Pinned tools prepare independently via `try_join!`. The catalog profile/home lock serializes operations sharing the profile. The adapter builds PATH from the selected compiler, cached tools, optional MSYS host tools, and inherited PATH. It removes child CC/CXX and declares compiler executables in the provider profile.

Before install, `conan cache check-integrity '*:*'` verifies provider manifests. Installation uses explicit host/build profiles, `--build=missing` or `--build=*`, `PkgConfigDeps`, `full_deploy`, JSON graph output, and an upstream lock output. Matching root declarations reuse an embedded revision lock unless an explicit lock refresh is requested. Offline sets `--no-remote`. Empty root sets use a temporary empty `conanfile.txt`.

After install, cache integrity runs again. `prepare_metadata` filters host nodes with deployed folders, enforces containment, collects files/DLLs, obtains module names and aliases from `cpp_info`, relocates generated `.pc` content, and calls shared linking/profile/inspection code. It rewrites `.cmake` stage paths, writes local graph diagnostics, creates a SHA-256 deployment inventory, then commits environment, manifest, lock, toolchain JSON, and activation. Compiler/cache paths remain local rather than inside the portable lock. Provider subprocess behavior is external and is not reconstructed as first-party code.

## Removal

```text
cpuv rm alpha beta
  -> Clap Action::Remove (visible alias "rm")
  -> execute: sync(root, cli, Some((packages, true)), false)
  -> remove each exact direct manifest key from the in-memory copy
  -> validate remaining roots and resolve/rebuild through the selected backend
  -> stage a graph containing only reachable remaining packages
  -> commit environment + manifest + lock
```

No package-specific uninstall script executes. Shared cache bytes are retained. If a remaining root still requires a package, it stays. A non-direct name aborts before saving any edited manifest, including when earlier names in the same invocation were valid. Explicit `[link]` selections can make a removal fail if they still reference an interface removed from the graph. The old `cpuv remove` command remains valid. This removes project dependencies, not the cpuv application or MSYS2 host tooling.

## Compilation and execution

`execute` may first upgrade legacy link metadata. `runner::run` reads the environment and composes PATH. Other project commands run directly with the project working directory. Compiler-like names enter a build lock and `plan`.

`plan` separates application arguments after `--`, rejects response files, classifies query/compile/link modes, rewrites explicit/default output basenames into `.cpuv/build`, handles supported auxiliary linker outputs, and emits separate commands for multiple compile-only sources. Duplicate output basenames fail. `compiler_args` validates schema/profile, preserves queries, adds the standard if appropriate, include paths, definitions and public compile flags, and appends libraries/driver flags only for link operations. Managed MSYS2 links use GNU library groups; catalog source-origin plans do not add these groups automatically.

Compiler diagnostics are streamed through a thread while retaining up to 2 MiB for missing-header help. A nonzero compile result returns immediately and never launches a stale executable. Successful program links execute unless `--no-run`; the child keeps stdin/stdout, receives runtime PATH and project CWD, and its exit code returns to the user. `main` calls `process::exit`, so not every live Rust stack local is guaranteed to run a destructor at that final point; resource scopes inside the completed workflows matter.

## Installation and updating

Native setup stages a copy into a user directory and optionally updates the current user's registry PATH, retaining its string type and backing up the original. The PowerShell bootstrap either invokes a colocated portable binary or downloads setup/checksum assets. Self update validates a stable version, follows policy-checked release redirects, compares versions, locks the executable directory, bounds and hashes downloaded assets, executes a candidate's `--version`, then calls `self_replace`. That candidate is executable code from the trusted release channel, not a sandboxed parser.

Sources: [main.rs](../../crates/cpuv-cli/src/main.rs), [source.rs](../../crates/cpuv-cli/src/source.rs), [runner.rs](../../crates/cpuv-cli/src/runner.rs), [solver](../../crates/cpuv-solver/src/lib.rs), [updater](../../crates/cpuv-cli/src/update.rs), [installer](../../crates/cpuv-cli/src/install.rs).
