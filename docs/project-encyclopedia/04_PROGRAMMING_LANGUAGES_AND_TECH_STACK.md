# 04 · Languages and technology stack

[Index](MASTER_INDEX.md) · [Exact dependency catalog](21_REFERENCE_CATALOG.md)

## Languages and process boundaries

Rust implements the application: seven crates, 19 source files, and 6,003 source lines in the inspected baseline. PowerShell provides four installation/build/package scripts totaling 196 lines. The release workflow includes a Bash block for GitHub publication. Packaging generates a small Windows CMD installer entry point. C++ is the user's target language and also appears in the generated doctor probe; it is not a separate first-party C++ implementation tree. Markdown/Mermaid document the system; TOML, JSON, and YAML configure or serialize it.

Counts are physical source lines, including comments/blank lines, not percentages of executed code. Cargo.lock and docs are not counted as programming languages. See [coverage](COVERAGE_MATRIX.md) for the file inventory.

Rust libraries communicate through typed calls and serialized filesystem records. Rust invokes GCC/Conan/build tools as processes. Windows-specific `windows-sys` bindings and a small `extern "system"` kernel32 call provide registry, console, quoting, messaging, and hidden-file operations. Conan's standalone distribution bundles its own Python runtime; no Python source runtime is embedded in the cpuv executable. Its upstream recipe execution is outside the first-party code boundary.

## Technology inventory

Versions below come from [workspace manifest](../../Cargo.toml), per-crate manifests, [Cargo.lock](../../Cargo.lock), and [build-tools.toml](../../build-tools.toml). All listed first-party integrations are IMPLEMENTED; version compatibility beyond executed checks remains bounded.

| Technology | Constraint → locked version | Purpose and principal location |
| --- | --- | --- |
| Rust | edition 2024; minimum 1.88 | All crates; actual audit compiler 1.99.0 |
| Cargo | resolver 3 | Workspace dependency/build manager; audit Cargo 1.99.0 |
| anyhow | 1 → 1.0.104 | Contextual error propagation across crates |
| serde / serde_json | 1 → 1.0.229 / 1.0.154 | TOML/JSON model serialization and provider graph parsing |
| clap | 4 → 4.6.7 | CLI derives, environment options, command/alias parsing |
| toml | 0.8 → 0.8.23 | Project/lock/tool manifest parsing |
| tokio | 1 → 1.53.3 | Runtime, async network/file operations, blocking task pool |
| reqwest | 0.12 → 0.12.28 | HTTP clients; default features disabled, rustls TLS, streaming, HTTP/2 |
| futures-util | 0.3 → 0.3.34 | Buffered concurrent download/extraction futures |
| sha2 | 0.10 → 0.10.9 | Archive/tool/update SHA-256 and deployed inventories |
| blake3 | 1 → 1.8.7 | CAS file identities, ABI/profile keys |
| fs2 | 0.4 → 0.4.3 | Project/package/build/catalog/update locks |
| tempfile | 3 → 3.27.0 | Download files, staging directories, doctor scratch space |
| walkdir | 2 → 2.5.0 | Payload, metadata, and binary traversal |
| tar / zstd / flate2 | 0.4 / 0.13 / 1 → 0.4.46 / 0.13.3 / 1.1.10 | MSYS2 package/index and generic tool archive decoding |
| zip | 2.4 → 2.6.1 | ZIP bootstrap extraction, deflate feature |
| regex | 1 → 1.13.1 | GCC/version/metadata parsing and name comparison support |
| indicatif | 0.17 → 0.17.11 | MSYS2 transfer progress bars |
| shlex | direct 1.3 → 1.3.0 | Public pkg-config tokenization; 2.0.1 is also transitively locked |
| goblin | 0.10 → 0.10.7 | PE/import/export and archive inspection, raw-shim DLL traversal |
| self-replace | 1.5 → 1.5.0 | Windows-safe replacement of the invoked executable |
| windows-sys | direct 0.61 → 0.61.2 | Win32 FFI; other locked versions are transitive |
| Conan | pinned 2.33.0 | External dependency graph/build/package metadata provider |
| CMake | bootstrap pinned 3.31.6 | Generic configure/build helper; upstream recipes may request other versions |
| Ninja | bootstrap pinned 1.12.1 | Parallel native build executor |
| GCC / linker | discovered, not bundled or pinned globally | Compile, assemble, link; actual capabilities are probed |
| MSYS2 | rolling provider, no global version | UCRT64 snapshot metadata and compatible binary packages |
| GitHub Actions | checkout@v4, rust-toolchain@stable, rust-cache@v2, upload/download-artifact@v4 | Windows checks/builds and release publication |

## Build/runtime implications and rationale

**Fact:** release builds use thin LTO, stripping, one codegen unit, and `+crt-static` for x64 Windows MSVC. The compiler-shim logic is linked into the principal CLI. **Inference:** this supports a small standalone deployment surface and avoids a separate Microsoft runtime installer, at the cost of longer release linking and a shared executable dispatch contract.

**Fact:** Tokio handles concurrent network futures while extraction is scheduled separately. **Inference:** bounded concurrency limits sockets, blocking jobs, and memory rather than assuming arbitrary parallelism improves performance. Process-based Conan integration reuses community build metadata but introduces external version/schema and executable trust dependencies.

No frontend framework, ORM, message broker, container platform, database engine, or first-party Python/JavaScript application was found. Cargo dependency versions are build inputs, not a vulnerability assessment. The declared minimum Rust version was not independently tested; the successful audit build used 1.99.0. See [quality](11_TESTING_AND_QUALITY.md).
