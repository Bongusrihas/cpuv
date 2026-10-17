# Contributing

Use stable Rust 1.88 or newer and the Microsoft C++ build tools required by Rust's Windows MSVC toolchain. GCC is needed to use cpuv for C++ compilation; a compatible MSYS2 UCRT64 compiler is required for managed binary dependencies. Keep changes focused and explain observable behavior in pull requests.

Other native GCC profiles use recipe-backed source dependencies. Maintain pinned URLs and SHA-256 values in recipes.toml, retain upstream licenses, and describe compiler/language/tool requirements. Review public pkg-config interfaces and transitive dependencies rather than guessing links from headers. Source recipes execute upstream build code; they must not override the selected compiler or saved ABI profile. See [source builds](docs/source-builds.md).

Before submitting:

~~~powershell
cargo fmt --all -- --check
cargo clippy --workspace --locked -- -D warnings
cargo build --workspace --locked
~~~

Use scripts/build.ps1 for a release build, scripts/install.ps1 for developer installation, and scripts/package.ps1 to create standalone release assets. The Cargo target directory is discovered through Cargo metadata, so CARGO_TARGET_DIR is supported. See [release instructions](docs/releases.md).

All first-party runtime code lives under crates/, with the embedded source registry in recipes.toml. The compiler shim, download/store pipeline, source backend, resolver, metadata linking, binary inspection, native installer, and updater are included. Third-party libraries are resolved from the manifests and Cargo.lock. This source distribution excludes test suites, simulations, recorded benchmarks, and generated documentation artifacts.

Do not commit generated environments, build outputs, caches, binaries, local reports, absolute personal paths, or credentials. Commit Cargo.lock for reproducible application builds. Preserve upstream license notices. Document ABI or provider limitations explicitly; do not bypass compatibility checks to make an incompatible package appear supported.

The Windows CI workflow checks source formatting, Clippy, and compilation. The release workflow builds and packages the prebuilt executable. These checks do not establish universal GCC, linker, or ABI compatibility; the [compatibility policy](docs/compatibility.md) defines the supported boundary.
