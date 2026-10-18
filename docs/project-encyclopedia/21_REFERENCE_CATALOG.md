# 21 · Reference catalog

[Index](MASTER_INDEX.md) · [File guide](05_REPOSITORY_AND_FILE_BY_FILE_GUIDE.md) · [Configuration](12_CONFIGURATION_AND_ENVIRONMENT.md)

This catalog is generated from the inspected post-feature source inventory and Cargo.lock. Line numbers identify this snapshot, not a permanent API guarantee. Local functions are included as navigation aids, not promised public crate interfaces. Narrative behavior is in chapters 03–09.

## Quick lookup

| Task/concept | Entry or record | Reference |
| --- | --- | --- |
| CLI commands and aliases | Cli, Action, execute | [Interfaces](08_API_AND_COMMUNICATION_PROTOCOLS.md) |
| Compiler discovery/doctor | discover, probe_with_flags, doctor | [Walkthrough](06_CODE_WALKTHROUGH.md) |
| MSYS2 graph | Index, Package, resolve_for | [Architecture](03_ARCHITECTURE_DEEP_DIVE.md) |
| Catalog graph/build | source::sync, CatalogLock, configure_profile | [Walkthrough](06_CODE_WALKTHROUGH.md) |
| Project dependencies | cpuv.toml, cpuv.lock | [Storage](07_DATA_MODEL_AND_STORAGE.md) |
| Runtime compiler interface | Environment, compiler_args | [Storage](07_DATA_MODEL_AND_STORAGE.md) |
| Link ownership | Selection, PackageFiles, linking::apply | [File guide](05_REPOSITORY_AND_FILE_BY_FILE_GUIDE.md) |
| Package removal | Action::Remove; visible alias rm | [Removal trace](06_CODE_WALKTHROUGH.md#removal) |
| Integrity/trust | safe_relative, cache checks, policy | [Security](09_SECURITY_AND_TRUST_MODEL.md) |
| Behavior checks | External six-scenario removal harness | [Quality](11_TESTING_AND_QUALITY.md) |
| Server/database API | NOT FOUND in first-party source | [Interfaces](08_API_AND_COMMUNICATION_PROTOCOLS.md) |
| Setup/ZIP/checksums | scripts/package.ps1; ignored dist | [Build/deploy](10_BUILD_RUN_TEST_AND_DEPLOY.md) |

## First-party symbol index

The table includes functions, structs, enums, and type aliases detected from source declarations. It does not parse macro expansion, impl trait resolution, or generated Clap/serde code. The compiler build validates Rust syntax; this index is a lexical navigation aid.

### `crates/cpuv-cli/src/host_tools.rs`

[Source](../../crates/cpuv-cli/src/host_tools.rs) · 77 physical lines.

| Symbol | Source line |
| --- | --- |
| `Manager` | 9 |
| `discover` | 15 |
| `command` | 36 |
| `ensure_base_devel` | 47 |

### `crates/cpuv-cli/src/install.rs`

[Source](../../crates/cpuv-cli/src/install.rs) · 264 physical lines.

| Symbol | Source line |
| --- | --- |
| `Options` | 7 |
| `Setup` | 22 |
| `is_setup` | 27 |
| `setup` | 37 |
| `install` | 57 |
| `wide` | 113 |
| `Key` | 116 |
| `drop` | 118 |
| `open` | 125 |
| `read` | 149 |
| `write` | 194 |
| `prepend` | 217 |
| `add_to_user_path` | 232 |

### `crates/cpuv-cli/src/main.rs`

[Source](../../crates/cpuv-cli/src/main.rs) · 1066 physical lines.

| Symbol | Source line |
| --- | --- |
| `Cli` | 31 |
| `transfer_limits` | 83 |
| `Action` | 95 |
| `SelfAction` | 149 |
| `Project` | 157 |
| `default_standard` | 163 |
| `Dependency` | 168 |
| `version` | 177 |
| `PinnedToolchain` | 191 |
| `Manifest` | 197 |
| `link_selection_empty` | 207 |
| `Backend` | 212 |
| `backend_is_auto` | 218 |
| `Lock` | 222 |
| `write_atomic` | 230 |
| `project_lock` | 239 |
| `read_manifest` | 252 |
| `check_toolchain` | 267 |
| `requirements` | 279 |
| `install_shim` | 310 |
| `activation` | 322 |
| `init` | 335 |
| `validate_packages` | 412 |
| `add` | 430 |
| `sync` | 486 |
| `Report` | 801 |
| `cache_dir` | 854 |
| `configure_environment` | 864 |
| `main` | 922 |
| `execute` | 949 |

### `crates/cpuv-cli/src/runner.rs`

[Source](../../crates/cpuv-cli/src/runner.rs) · 481 physical lines.

| Symbol | Source line |
| --- | --- |
| `state_directory` | 12 |
| `SetFileAttributesW` | 20 |
| `compiler_command` | 36 |
| `source` | 47 |
| `link_input` | 54 |
| `artifact` | 61 |
| `takes_value` | 73 |
| `Plan` | 96 |
| `plan` | 102 |
| `runtime_path` | 286 |
| `status` | 301 |
| `missing_header_help` | 309 |
| `compiler_status` | 322 |
| `run` | 352 |
| `clean` | 430 |

### `crates/cpuv-cli/src/source.rs`

[Source](../../crates/cpuv-cli/src/source.rs) · 808 physical lines.

| Symbol | Source line |
| --- | --- |
| `Archive` | 7 |
| `one` | 18 |
| `Tool` | 22 |
| `Registry` | 29 |
| `custom_profile_requested` | 32 |
| `Profile` | 38 |
| `split_flags` | 46 |
| `split_flags` | 79 |
| `environment` | 84 |
| `compiler_flags` | 100 |
| `CatalogLock` | 119 |
| `requests_locked` | 130 |
| `standard` | 148 |
| `reference` | 185 |
| `command` | 212 |
| `run` | 222 |
| `configure_profile` | 250 |
| `modules_for` | 325 |
| `prepare_metadata` | 356 |
| `sync` | 505 |
| `SavedProfile` | 519 |
| `info` | 739 |
| `commit` | 752 |

### `crates/cpuv-cli/src/source/archive.rs`

[Source](../../crates/cpuv-cli/src/source/archive.rs) · 240 physical lines.

| Symbol | Source line |
| --- | --- |
| `digest` | 5 |
| `download` | 19 |
| `destination` | 91 |
| `selected` | 100 |
| `extract` | 114 |
| `inventory` | 212 |
| `verify` | 230 |

### `crates/cpuv-cli/src/source/tools.rs`

[Source](../../crates/cpuv-cli/src/source/tools.rs) · 99 physical lines.

| Symbol | Source line |
| --- | --- |
| `Tools` | 3 |
| `get` | 9 |
| `prepare` | 82 |

### `crates/cpuv-cli/src/update.rs`

[Source](../../crates/cpuv-cli/src/update.rs) · 204 physical lines.

| Symbol | Source line |
| --- | --- |
| `Options` | 15 |
| `version` | 34 |
| `checksum` | 46 |
| `download` | 58 |
| `run` | 95 |

### `crates/cpuv-fetch/src/lib.rs`

[Source](../../crates/cpuv-fetch/src/lib.rs) · 360 physical lines.

| Symbol | Source line |
| --- | --- |
| `Fetcher` | 24 |
| `DownloadStats` | 36 |
| `new` | 45 |
| `with_limits` | 58 |
| `with_namespace` | 87 |
| `index` | 93 |
| `download_all` | 163 |
| `Guard` | 199 |
| `drop` | 201 |
| `download_one` | 251 |
| `verify_file` | 293 |
| `parse_database` | 315 |
| `parse_database_for` | 319 |
| `parse_database_with_limits` | 323 |

### `crates/cpuv-fetch/src/policy.rs`

[Source](../../crates/cpuv-fetch/src/policy.rs) · 63 physical lines.

| Symbol | Source line |
| --- | --- |
| `Limits` | 6 |
| `default` | 16 |
| `allowed_url` | 27 |
| `client` | 39 |

### `crates/cpuv-relocate/src/inspect.rs`

[Source](../../crates/cpuv-relocate/src/inspect.rs) · 151 physical lines.

| Symbol | Source line |
| --- | --- |
| `Binary` | 11 |
| `environment` | 18 |
| `environment_with_runtime_policy` | 22 |

### `crates/cpuv-relocate/src/lib.rs`

[Source](../../crates/cpuv-relocate/src/lib.rs) · 380 physical lines.

| Symbol | Source line |
| --- | --- |
| `Environment` | 14 |
| `msys_host_bin` | 41 |
| `expand` | 46 |
| `replace_private` | 60 |
| `index_environment` | 69 |
| `portable` | 159 |
| `generate_specs` | 165 |
| `is_compiler_query` | 205 |
| `validate_profile` | 224 |
| `compiler_args` | 317 |

### `crates/cpuv-relocate/src/linking.rs`

[Source](../../crates/cpuv-relocate/src/linking.rs) · 277 physical lines.

| Symbol | Source line |
| --- | --- |
| `Selection` | 13 |
| `PackageFiles` | 23 |
| `Module` | 29 |
| `read` | 34 |
| `words` | 61 |
| `requirement_modules` | 89 |
| `relative` | 139 |
| `unique` | 155 |
| `apply` | 161 |

### `crates/cpuv-shim/src/lib.rs`

[Source](../../crates/cpuv-shim/src/lib.rs) · 161 physical lines.

| Symbol | Source line |
| --- | --- |
| `environment` | 11 |
| `output_path` | 28 |
| `deploy_dlls` | 62 |
| `run` | 124 |

### `crates/cpuv-shim/src/main.rs`

[Source](../../crates/cpuv-shim/src/main.rs) · 9 physical lines.

| Symbol | Source line |
| --- | --- |
| `main` | 1 |

### `crates/cpuv-solver/src/lib.rs`

[Source](../../crates/cpuv-solver/src/lib.rs) · 382 physical lines.

| Symbol | Source line |
| --- | --- |
| `Package` | 14 |
| `Index` | 25 |
| `parse_desc` | 27 |
| `normalize` | 78 |
| `normalize_for` | 82 |
| `split_requirement` | 89 |
| `segments_cmp` | 102 |
| `version_cmp` | 185 |
| `epoch` | 186 |
| `matches` | 203 |
| `suggestions` | 217 |
| `distance` | 218 |
| `provider` | 262 |
| `resolve` | 316 |
| `available` | 320 |
| `resolve_for` | 331 |
| `visit` | 332 |

### `crates/cpuv-store/src/lib.rs`

[Source](../../crates/cpuv-store/src/lib.rs) · 346 physical lines.

| Symbol | Source line |
| --- | --- |
| `StoredFile` | 12 |
| `StoredPackage` | 18 |
| `Store` | 23 |
| `LinkStats` | 29 |
| `safe_relative` | 35 |
| `link_or_copy` | 64 |
| `pacman_metadata` | 77 |
| `digest_file` | 88 |
| `new` | 103 |
| `with_extraction_limits` | 113 |
| `package_dir` | 118 |
| `cached` | 121 |
| `ingest` | 150 |
| `materialize` | 314 |

### `crates/cpuv-toolchain/src/lib.rs`

[Source](../../crates/cpuv-toolchain/src/lib.rs) · 422 physical lines.

| Symbol | Source line |
| --- | --- |
| `AbiTuple` | 13 |
| `Provider` | 30 |
| `msys2` | 38 |
| `Toolchain` | 49 |
| `driver_path` | 67 |
| `is_project_shim` | 74 |
| `locate` | 78 |
| `discover` | 105 |
| `field` | 120 |
| `macro_value` | 125 |
| `parse_verbose` | 128 |
| `runtime_tag` | 271 |
| `default_standard` | 277 |
| `compiler_command` | 285 |
| `probe` | 298 |
| `probe_with_flags` | 302 |
| `doctor` | 388 |

### `crates/cpuv-toolchain/src/msys.rs`

[Source](../../crates/cpuv-toolchain/src/msys.rs) · 213 physical lines.

| Symbol | Source line |
| --- | --- |
| `add_root` | 5 |
| `discover` | 14 |
| `Key` | 104 |
| `drop` | 106 |
| `wide` | 112 |
| `open` | 115 |
| `text` | 121 |
| `installations` | 161 |

## Direct component dependencies

Dependencies are manifest declarations, not runtime processes unless the implementation launches them. Workspace-level libraries are inherited into each crate.

| Crate | Manifest | Direct Rust dependency names |
| --- | --- | --- |
| cpuv-cli | [manifest](../../crates/cpuv-cli/Cargo.toml) | `anyhow`, `blake3`, `clap`, `cpuv-fetch`, `cpuv-relocate`, `cpuv-shim`, `cpuv-solver`, `cpuv-store`, `cpuv-toolchain`, `flate2`, `fs2`, `futures-util`, `reqwest`, `self-replace`, `serde`, `serde_json`, `sha2`, `tar`, `tempfile`, `tokio`, `toml`, `walkdir`, `windows-sys`, `zip` |
| cpuv-fetch | [manifest](../../crates/cpuv-fetch/Cargo.toml) | `anyhow`, `cpuv-solver`, `flate2`, `futures-util`, `indicatif`, `reqwest`, `serde`, `sha2`, `tar`, `tempfile`, `tokio`, `zstd` |
| cpuv-relocate | [manifest](../../crates/cpuv-relocate/Cargo.toml) | `anyhow`, `cpuv-solver`, `goblin`, `regex`, `serde`, `serde_json`, `shlex`, `walkdir` |
| cpuv-shim | [manifest](../../crates/cpuv-shim/Cargo.toml) | `anyhow`, `cpuv-relocate`, `goblin`, `serde_json` |
| cpuv-solver | [manifest](../../crates/cpuv-solver/Cargo.toml) | `anyhow`, `regex`, `serde` |
| cpuv-store | [manifest](../../crates/cpuv-store/Cargo.toml) | `anyhow`, `blake3`, `cpuv-solver`, `fs2`, `serde`, `serde_json`, `tar`, `tempfile`, `walkdir`, `zstd` |
| cpuv-toolchain | [manifest](../../crates/cpuv-toolchain/Cargo.toml) | `anyhow`, `blake3`, `regex`, `serde`, `serde_json`, `tempfile`, `windows-sys` |

## Complete locked package-version catalog

Cargo.lock contains 228 package records, including first-party workspace entries. Multiple versions of one crate are separate dependency identities. This inventories resolved build inputs; it is not a vulnerability, license compatibility, or upstream-source audit. Source dependency edges are available in Cargo.lock itself.

| Package | Locked version |
| --- | --- |
| `adler2` | `2.0.1` |
| `aho-corasick` | `1.1.5` |
| `anstream` | `1.0.0` |
| `anstyle` | `1.0.14` |
| `anstyle-parse` | `1.0.0` |
| `anstyle-query` | `1.1.5` |
| `anstyle-wincon` | `3.0.11` |
| `anyhow` | `1.0.104` |
| `arbitrary` | `1.5.0` |
| `arrayvec` | `0.7.8` |
| `atomic-waker` | `1.1.2` |
| `base64` | `0.22.1` |
| `base64` | `0.23.1` |
| `bitflags` | `2.13.2` |
| `blake3` | `1.8.7` |
| `block-buffer` | `0.10.4` |
| `bumpalo` | `3.20.3` |
| `bytes` | `1.12.1` |
| `cc` | `1.8.0` |
| `cfg-if` | `1.0.5` |
| `cfg_aliases` | `0.2.2` |
| `chacha20` | `0.10.2` |
| `clap` | `4.6.7` |
| `clap_builder` | `4.6.7` |
| `clap_derive` | `4.6.7` |
| `clap_lex` | `1.1.1` |
| `colorchoice` | `1.0.5` |
| `console` | `0.15.11` |
| `constant_time_eq` | `0.4.2` |
| `cpufeatures` | `0.2.17` |
| `cpufeatures` | `0.3.1` |
| `cpuv-cli` | `0.0.2` |
| `cpuv-fetch` | `0.0.2` |
| `cpuv-relocate` | `0.0.2` |
| `cpuv-shim` | `0.0.2` |
| `cpuv-solver` | `0.0.2` |
| `cpuv-store` | `0.0.2` |
| `cpuv-toolchain` | `0.0.2` |
| `crc32fast` | `1.5.2` |
| `crossbeam-utils` | `0.8.23` |
| `crypto-common` | `0.1.7` |
| `derive_arbitrary` | `1.5.0` |
| `digest` | `0.10.7` |
| `displaydoc` | `0.2.7` |
| `encode_unicode` | `1.0.0` |
| `equivalent` | `1.0.3` |
| `errno` | `0.3.14` |
| `fastrand` | `2.5.0` |
| `filetime` | `0.2.29` |
| `find-msvc-tools` | `0.1.14` |
| `flate2` | `1.1.10` |
| `fnv` | `1.0.7` |
| `form_urlencoded` | `1.2.2` |
| `fs2` | `0.4.3` |
| `futures-channel` | `0.3.34` |
| `futures-core` | `0.3.34` |
| `futures-io` | `0.3.34` |
| `futures-macro` | `0.3.34` |
| `futures-sink` | `0.3.34` |
| `futures-task` | `0.3.34` |
| `futures-util` | `0.3.34` |
| `generic-array` | `0.14.7` |
| `getrandom` | `0.2.17` |
| `getrandom` | `0.4.3` |
| `goblin` | `0.10.7` |
| `h2` | `0.4.20` |
| `hashbrown` | `0.17.1` |
| `heck` | `0.5.0` |
| `http` | `1.5.0` |
| `http-body` | `1.1.0` |
| `http-body-util` | `0.1.5` |
| `httparse` | `1.10.1` |
| `hyper` | `1.12.0` |
| `hyper-rustls` | `0.27.10` |
| `hyper-util` | `0.1.21` |
| `icu_collections` | `2.3.0` |
| `icu_locale_core` | `2.3.0` |
| `icu_normalizer` | `2.3.0` |
| `icu_normalizer_data` | `2.3.0` |
| `icu_properties` | `2.3.0` |
| `icu_properties_data` | `2.3.0` |
| `icu_provider` | `2.3.1` |
| `idna` | `1.1.0` |
| `idna_adapter` | `1.2.2` |
| `indexmap` | `2.14.2` |
| `indicatif` | `0.17.11` |
| `ipnet` | `2.12.2` |
| `is_terminal_polyfill` | `1.70.2` |
| `itoa` | `1.0.18` |
| `jobserver` | `0.1.35` |
| `js-sys` | `0.3.106` |
| `libc` | `0.2.190` |
| `linux-raw-sys` | `0.12.1` |
| `litemap` | `0.8.3` |
| `log` | `0.4.34` |
| `lru-slab` | `0.1.3` |
| `memchr` | `2.8.3` |
| `miniz_oxide` | `0.9.1` |
| `mio` | `1.2.4` |
| `number_prefix` | `0.4.0` |
| `once_cell` | `1.21.4` |
| `once_cell_polyfill` | `1.70.2` |
| `percent-encoding` | `2.3.2` |
| `pin-project-lite` | `0.2.17` |
| `pkg-config` | `0.3.34` |
| `plain` | `0.2.3` |
| `portable-atomic` | `1.15.0` |
| `potential_utf` | `0.1.6` |
| `proc-macro2` | `1.0.107` |
| `quinn` | `0.11.12` |
| `quinn-proto` | `0.11.19` |
| `quinn-udp` | `0.5.16` |
| `quote` | `1.0.47` |
| `r-efi` | `6.0.0` |
| `rand` | `0.10.3` |
| `rand_core` | `0.10.1` |
| `rand_pcg` | `0.10.2` |
| `regex` | `1.13.1` |
| `regex-automata` | `0.4.18` |
| `regex-syntax` | `0.8.11` |
| `reqwest` | `0.12.28` |
| `ring` | `0.17.14` |
| `rustc-hash` | `2.1.3` |
| `rustix` | `1.1.5` |
| `rustls` | `0.23.46` |
| `rustls-pki-types` | `1.15.1` |
| `rustls-webpki` | `0.103.15` |
| `rustversion` | `1.0.23` |
| `ryu` | `1.0.23` |
| `same-file` | `1.0.6` |
| `scroll` | `0.13.0` |
| `scroll_derive` | `0.13.2` |
| `self-replace` | `1.5.0` |
| `serde` | `1.0.229` |
| `serde_core` | `1.0.229` |
| `serde_derive` | `1.0.229` |
| `serde_json` | `1.0.154` |
| `serde_spanned` | `0.6.9` |
| `serde_urlencoded` | `0.7.1` |
| `sha2` | `0.10.9` |
| `shlex` | `1.3.0` |
| `shlex` | `2.0.1` |
| `simd-adler32` | `0.3.10` |
| `slab` | `0.4.12` |
| `smallvec` | `1.16.3` |
| `socket2` | `0.6.5` |
| `stable_deref_trait` | `1.2.1` |
| `strsim` | `0.11.1` |
| `subtle` | `2.6.1` |
| `syn` | `3.0.9` |
| `sync_wrapper` | `1.0.2` |
| `synstructure` | `0.14.0` |
| `tar` | `0.4.46` |
| `tempfile` | `3.27.0` |
| `thiserror` | `2.0.21` |
| `thiserror-impl` | `2.0.21` |
| `tinystr` | `0.8.4` |
| `tinyvec` | `1.13.3` |
| `tokio` | `1.53.3` |
| `tokio-macros` | `2.7.2` |
| `tokio-rustls` | `0.26.6` |
| `tokio-util` | `0.7.20` |
| `toml` | `0.8.23` |
| `toml_datetime` | `0.6.11` |
| `toml_edit` | `0.22.27` |
| `toml_write` | `0.1.2` |
| `tower` | `0.5.3` |
| `tower-http` | `0.6.11` |
| `tower-layer` | `0.3.3` |
| `tower-service` | `0.3.3` |
| `tracing` | `0.1.44` |
| `tracing-core` | `0.1.36` |
| `try-lock` | `0.2.5` |
| `typenum` | `1.20.1` |
| `unicode-ident` | `1.0.27` |
| `unicode-width` | `0.2.2` |
| `untrusted` | `0.9.0` |
| `url` | `2.5.8` |
| `utf8_iter` | `1.0.4` |
| `utf8parse` | `0.2.2` |
| `version_check` | `0.9.5` |
| `walkdir` | `2.5.0` |
| `want` | `0.3.2` |
| `wasi` | `0.11.1+wasi-snapshot-preview1` |
| `wasm-bindgen` | `0.2.129` |
| `wasm-bindgen-futures` | `0.4.79` |
| `wasm-bindgen-macro` | `0.2.129` |
| `wasm-bindgen-macro-support` | `0.2.129` |
| `wasm-bindgen-shared` | `0.2.129` |
| `wasm-streams` | `0.4.2` |
| `web-sys` | `0.3.106` |
| `web-time` | `1.1.0` |
| `webpki-roots` | `1.0.9` |
| `winapi` | `0.3.9` |
| `winapi-i686-pc-windows-gnu` | `0.4.0` |
| `winapi-util` | `0.1.11` |
| `winapi-x86_64-pc-windows-gnu` | `0.4.0` |
| `windows-link` | `0.2.1` |
| `windows-sys` | `0.52.0` |
| `windows-sys` | `0.59.0` |
| `windows-sys` | `0.61.2` |
| `windows-targets` | `0.52.6` |
| `windows_aarch64_gnullvm` | `0.52.6` |
| `windows_aarch64_msvc` | `0.52.6` |
| `windows_i686_gnu` | `0.52.6` |
| `windows_i686_gnullvm` | `0.52.6` |
| `windows_i686_msvc` | `0.52.6` |
| `windows_x86_64_gnu` | `0.52.6` |
| `windows_x86_64_gnullvm` | `0.52.6` |
| `windows_x86_64_msvc` | `0.52.6` |
| `winnow` | `0.7.15` |
| `writeable` | `0.6.4` |
| `xattr` | `1.6.1` |
| `yoke` | `0.8.3` |
| `yoke-derive` | `0.8.4` |
| `zerofrom` | `0.1.8` |
| `zerofrom-derive` | `0.1.8` |
| `zeroize` | `1.9.1` |
| `zerotrie` | `0.2.5` |
| `zerovec` | `0.11.8` |
| `zerovec-derive` | `0.11.6` |
| `zip` | `2.6.1` |
| `zlib-rs` | `0.6.8` |
| `zmij` | `1.0.23` |
| `zopfli` | `0.8.4` |
| `zstd` | `0.13.3` |
| `zstd-safe` | `7.3.0` |
| `zstd-sys` | `2.1.2+zstd.1.5.7` |

## Interface and artifact references

- [Complete configuration flags, environment names, and manifest keys](12_CONFIGURATION_AND_ENVIRONMENT.md).
- [Filesystem schema, cache keys, concurrency locks, and retention](07_DATA_MODEL_AND_STORAGE.md).
- [Command/process/HTTP interfaces and error conventions](08_API_AND_COMMUNICATION_PROTOCOLS.md).
- [Glossary](17_GLOSSARY_AND_CONCEPTUAL_FOUNDATIONS.md).
- [File coverage and audit limits](COVERAGE_MATRIX.md).
- [Source hashes and lexical symbols](source-inventory.json).
