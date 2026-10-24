# Contributing

Use stable Rust 1.88 or newer and the Microsoft C++ build tools required by Rust's Windows MSVC toolchain. Native consumer checks use GCC, MSVC or GNU-style Clang. MSYS2 binary packages require their matching GCC environment; Conan and vcpkg use their declared compiler/provider profiles. Keep changes focused and explain observable behavior in pull requests.

Other supported native MinGW GCC profiles and missing MSYS2 packages use the generic ConanCenter adapter. Do not add library-name branches, embedded library recipes, aliases, or package-specific patches to cpuv. Package fixes belong in upstream catalog metadata. build-tools.toml pins only bootstrap tooling; update official URLs, versions, and SHA-256 values together. Review graph ownership, public interfaces, transitive dependencies, compiler-profile propagation, and catalog version compatibility. See [catalog installation](docs/source-builds.md).

Before submitting:

~~~powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
~~~

Use scripts/build.ps1 for a release build, scripts/install.ps1 for developer installation, and scripts/package.ps1 to create standalone release assets. The Cargo target directory is discovered through Cargo metadata, so CARGO_TARGET_DIR is supported. See [release instructions](docs/releases.md).

All first-party runtime code and retained regression tests live under crates/, with pinned bootstrap tools in build-tools.toml. The compiler shim, download/store pipeline, source backend, resolver, metadata linking, binary inspection, native installer, and updater are included. Third-party libraries are resolved from the manifests and Cargo.lock. New fixes must include behavioral coverage where practical. Use synthetic fixtures and isolated temporary projects; never run destructive installer or transaction tests against developer data.

Do not commit generated environments, build outputs, caches, binaries, local reports, absolute personal paths, or credentials. Commit Cargo.lock for reproducible application builds. Preserve upstream license notices. Document ABI or provider limitations explicitly; do not bypass compatibility checks to make an incompatible package appear supported.

The Windows CI and release workflows check formatting, Clippy, compilation, and the behavioral test suite. Tests establish only the cases and toolchains actually exercised; the [compatibility policy](docs/compatibility.md) defines the supported boundary.

## Phase 1 acceptance gate

Run `scripts/test-phase1.ps1 -Compiler C:\path\to\ucrt64\bin\g++.exe` before claiming Phase 1 complete. Add `-ReleaseBuild` to exercise optimized release binaries (required by the release workflow). It requires matching pkgconf, Rust/Git, and network access for the first Conan/tool bootstrap. All synthetic repositories, package corruption, transactions and installer destinations use isolated temporary directories; `test-install.ps1` uses `-NoPath -NoCompatCopy`.

The gate executes the ordinary workspace suite, then explicitly runs the native `tests/phase1.rs` fixture (normally ignored because it requires an external compiler). The crash helper marked ignored in `cpuv-core` is invoked by its parent test at all 32 initialization/update crash boundaries. These helpers must not be treated as unexecuted acceptance coverage. The gate also runs the real Conan shared/static/frozen/offline/repair contracts and the custom Cargo target installation test. `-SkipConan` is a partial development run and does not complete the gate.

The gate retains `evidence.json`, compiler identity and logs under a temporary evidence directory. CI and release workflows run the gate and upload its evidence. Provider receipts and local cache quarantine remain available for diagnosis; never commit their absolute paths or contents.
