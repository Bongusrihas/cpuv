# 02 · Requirements and domain model

[Index](MASTER_INDEX.md) · [Storage schemas](07_DATA_MODEL_AND_STORAGE.md)

This is a reconstruction of observable requirements, not an original signed requirements specification. **Fact** means source-supported behavior. **Inference** means an engineering interpretation. Original user conversations are not a versioned requirements artifact in this repository.

## Functional requirements traced to code

| ID | Requirement reconstructed from behavior | Implementation |
| --- | --- | --- |
| F1 | Create a manifest and machine-local compiler environment in a chosen folder | `init`, `runner::state_directory` |
| F2 | Resolve direct and transitive package requirements through one provider | `requirements`, `resolve_for`, `source::sync` |
| F3 | Validate MSYS2 package bytes before extracting and reusing them | `Fetcher::download_one`, `Store::ingest`, `Store::cached` |
| F4 | Supply compiler/linker interfaces from package ownership and public metadata | `linking::apply`, `compiler_args` |
| F5 | Compile and execute successful program links without root build artifacts | `runner::plan`, `runner::run` |
| F6 | Remove direct dependencies, pruning unreachable packages while retaining cache content | `Action::Remove`, `sync`; `rm` is a visible alias |
| F7 | Preserve active dependency state on ordinary prepare failures | Staging and commit paths in both backends |
| F8 | Work offline when required metadata, binaries, and tools are available | `Fetcher::index`, `download_all`, Conan `--no-remote` |
| F9 | Install and update a prebuilt application without compiling Rust on user machines | `install`, `update`, packaging scripts |
| F10 | Keep explicit host tooling separate from project library payload | `host_tools::Manager`, the `base-devel` split in `add` |

Sources: [main.rs](../../crates/cpuv-cli/src/main.rs), [runner.rs](../../crates/cpuv-cli/src/runner.rs), [source.rs](../../crates/cpuv-cli/src/source.rs), [store](../../crates/cpuv-store/src/lib.rs).

## Actors and ownership

The local user runs all operations with normal process permissions. Windows controls filesystem and registry access; cpuv has no application roles, login sessions, or remote authorization model. A maintainer builds and publishes release assets. MSYS2 and ConanCenter provide dependency metadata/content. GCC and upstream build scripts are executable dependencies, not passive data providers.

The user owns source files, manifest, lockfile, link selections, and compiler choice. cpuv owns generated `.cpuv` state and caches logically, but does not establish separate filesystem ACLs. Upstream publishers own package identities and compatibility claims. The updater changes the executable that was invoked, not every copy on disk.

## Domain entities

`Project` carries name, version, and language standard. `Manifest` combines it with `PinnedToolchain`, `Backend`, direct `Dependency` entries, and a `Selection` for linking. A dependency is either a version string or a detailed declaration with a version and optional linkage label. `Package` represents one MSYS2 snapshot entry with dependencies, virtual provides, conflicts, size, filename, and SHA-256. `StoredPackage` adds file paths, lengths, and BLAKE3 digests. `Toolchain` wraps observed compiler details and provider eligibility. `Environment` is the runtime compiler interface consumed by the runner and shim. `CatalogLock` stores provider revision references and saved profile inputs.

Relationships: one manifest has many direct requirements; a resolved graph adds transitive packages; many projects can reuse the same archive/blob; one environment has one recorded compiler profile; a link plan selects interfaces from installed package ownership.

## Invariants and rules

- MSYS2 names normalize to the selected namespace, without library-specific aliases. Conan names use its own exact catalog spelling.
- Version syntax is deliberately bounded: `*`, exact versions, or one comparison. MSYS2 compares pacman-style versions from one snapshot, not a full historical SemVer solution space.
- One environment uses one dependency provider. Fallback re-resolves the whole root set; equivalent names across ecosystems are not automatically translated.
- Only direct manifest keys can be removed. Removing a transitive-only package fails; it remains while a root requires it. An invalid member of a multi-package removal aborts before committing the edited manifest.
- Source includes do not select libraries. Ambiguous compiled interfaces require explicit `[link]` selection.
- Compiler, runtime, object format, and linker compatibility constrain support. Hash equality is not a universal ABI invariant.
- Hidden files are a user-interface convention, not an access-control boundary.

## Nonfunctional requirements and uncertainty

Bounded transfers, integrity checking, reuse, deterministic MSYS2 ordering, and recoverable normal-error transactions are implemented design goals. No fixed latency, memory budget for every parser, uptime objective, universal compatibility target, or complete hermeticity requirement is proven. Durability under abrupt termination and every rollback failure is incomplete. Explicit public/private static-link semantics and full catalog option control are also incomplete. See [decisions](13_ENGINEERING_DECISIONS_AND_TRADEOFFS.md) and [issues](19_KNOWN_ISSUES_AND_TECHNICAL_DEBT.md).
