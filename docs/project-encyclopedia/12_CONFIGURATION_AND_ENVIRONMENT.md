# 12 · Configuration and environment reference

[Index](MASTER_INDEX.md) · [Interfaces](08_API_AND_COMMUNICATION_PROTOCOLS.md) · [Storage](07_DATA_MODEL_AND_STORAGE.md)

## Resolution rules

Clap options with environment bindings accept explicit command-line values before their environment defaults. Ordinary project state comes from `cpuv.toml`; lock replay further constrains versions/repository/ABI. Runtime execution reads generated `Environment`. Catalog profiles merge current flags with saved values: absence retains saved flags, while `--reset-profile` discards them first. No generic configuration search path or user-wide cpuv TOML configuration file was found.

## Global CLI options

Source: [`Cli` and `transfer_limits`](../../crates/cpuv-cli/src/main.rs). All options are optional; defaults below are explicit source defaults. Invalid typed/ranged values fail parsing. Paths/URLs must also satisfy later filesystem or provider validation.

| Flag | Environment binding | Type, default, valid values and consumer |
| --- | --- | --- |
| `-C`, `--project` | None | Path, default `.`; ordinary command root is created/canonicalized |
| `--cache-dir` | CPUV_CACHE_DIR | Path; default `LOCALAPPDATA/cpuv`; required alternative if LOCALAPPDATA absent |
| `--repository` | CPUV_REPOSITORY | URL; MSYS2 override, else saved lock/provider/default; HTTPS or loopback HTTP |
| `--concurrent-downloads` | CPUV_CONCURRENT_DOWNLOADS | u16, 50, range 1–256; MSYS2 futures and Conan parallel binary setting |
| `--max-index-mib` | CPUV_MAX_INDEX_MIB | u64, 64, range 1–1,048,576; compressed MSYS2 metadata cap |
| `--max-download-mib` | CPUV_MAX_DOWNLOAD_MIB | u64, 16,384, same range; MSYS2/bootstrap archives only |
| `--max-extract-mib` | CPUV_MAX_EXTRACT_MIB | u64, 65,536, same range; expanded MSYS2/bootstrap payload |
| `--max-extract-files` | CPUV_MAX_EXTRACT_FILES | u64, 1,000,000, range 1–10,000,000; extraction entries |
| `--download-timeout-secs` | CPUV_DOWNLOAD_TIMEOUT_SECS | u64, 3,600; 0 disables whole-transfer deadline, retaining connect/read limits |
| `--offline` | None | Boolean false; first-party cached reads and Conan remotes disabled |
| `--refresh` | None | Boolean false; refreshes MSYS2 index. Catalog adapter does not directly consume this flag; see issues. |
| `--compiler` | None in Clap | Optional executable path; toolchain discovery then checks CPUV_CXX/CXX/PATH |
| `--from-source` | CPUV_FROM_SOURCE | Boolean false; selects catalog and forces all source builds including build requirements |
| `--catalog` | None | Boolean false; selects ConanCenter for whole graph |
| `--build-jobs` | CPUV_BUILD_JOBS | Optional u16, range 1–256; defaults to available parallelism or 1 for catalog helpers |
| `--reset-profile` | None | Boolean false; clears saved catalog flags before applying environment |

The MiB limits are converted to bytes. Decompressed MSYS2 index input is separately limited to sixteen times the compressed cap; individual desc entries are capped at 1 MiB. Conan recipe downloads/extraction and PowerShell bootstrap do not inherit these archive limits. The updater has its own fixed caps/timeouts.

## Project manifest

`Manifest` is required for project operations. `project.name`, `project.version`, and toolchain pin strings have no default when parsing an existing manifest. `project.std` defaults to `c++20` if omitted; initialization chooses either `default` or a detected supported GNU mode, so initialization and deserialization defaults differ.

| Key | Type/default | Validation and use |
| --- | --- | --- |
| `backend` | `auto` when omitted | `auto` or `conan`; legacy `source` deserializes as Conan |
| `project.name`, `project.version` | Required strings | Project metadata; project version is not cpuv's application version |
| `project.std` | String, parse default `c++20` | ISO/GNU C++98/03/11/14/17/20/23/26 or `default`; compiler must actually support chosen mode |
| `toolchain.target` | Required string | Compared to observed target triple |
| `toolchain.compiler` | Required `gcc@VERSION` string | Compared to observed GCC version |
| `toolchain.abi` | Required runtime-tag string | Compared to observed runtime/thread/exception tag, including one legacy spelling |
| `dependencies.NAME` | Version string, empty map default | Names restricted to alphanumeric and `-_.+`; catalog additionally requires lowercase naming |
| `dependencies.NAME.version` | Required in detailed form | `*`, exact, or one comparison; caret/tilde/comma/space range syntax rejected |
| `dependencies.NAME.linkage` | Optional string | Accepts absent/`shared`; provider/upstream metadata still decides actual artifacts, not a universal shared-option override |
| `link.pkg_config` | String list, empty | Explicit public module roots; replaces automatic selections |
| `link.libraries` | String list, empty | Additional validated library names without paths/leading flag |
| `link.flags` | String list, empty | Explicit trusted driver flags, later profile validation |

`Selection` denies unknown fields. The other manifest structs generally do not, so a typo outside `[link]` can be ignored by serde rather than rejected. No secret field is declared, but flags and URL values can contain sensitive user input and may reach logs.

Safe illustrative fragment, to be added to an initialized project:

```toml
[dependencies]
fmt = "*"

[link]
pkg_config = ["fmt"]
```

This is not a complete manifest: `cpuv init` supplies project/toolchain fields. Module/version names remain provider-specific.

## Environment variables beyond Clap

| Name | Read/write location | Meaning, missing/invalid behavior, sensitivity |
| --- | --- | --- |
| CPUV_CXX, CXX | `toolchain::discover` | Selected executable before PATH; empty value errors; executable only, no shell flags |
| PATH | Discovery and child process builders | Compiler/tool/DLL search; generated paths prepend inherited entries; executable trust boundary |
| CPUV_MSYS2_ROOT, MSYS2_ROOT, MSYSTEM_PREFIX, MINGW_PREFIX | `msys::discover` | Optional discovery hints; candidates still probed, no fixed installation directory |
| ProgramFiles, ProgramFiles(x86), USERPROFILE, SystemDrive | MSYS2 discovery; USERPROFILE also native setup | Optional bounded search parents; native default setup requires USERPROFILE |
| LOCALAPPDATA | `cache_dir` | Default shared cache base; missing without explicit cache fails |
| CPPFLAGS, CFLAGS, CXXFLAGS, LDFLAGS | `Profile::environment` | Saved trusted build input strings; Windows command-line parsing; invalid Unicode/quoting can fail; may contain sensitive values |
| CPUV_SHIM_TRACE | Runner and shim | Any present value enables diagnostic compiler invocation output |
| CPUV_ENV | Written by runner/activation; experimental specs read it | Generated environment path for child tools/specs; raw shim locates metadata independently |
| TMP, TEMP | Managed compiler child | Overridden to `.cpuv/build/tmp` |
| CONAN_HOME | Catalog child | Overridden to profile-keyed local home |
| CC, CXX | Catalog child | Removed so declared compiler executables govern provider build helpers; parent CXX still participates in initial discovery |
| CPUV_INSTALL_DIR | Native install and bootstrap | Optional destination; caller needs write permission |
| CPUV_VERSION | PowerShell bootstrap only | `latest` or allowed version/tag pattern; not an application runtime version override |
| CPUV_UPDATE_BASE_URL | Updater | Trusted release-base override; no query/fragment, policy-checked scheme |
| CARGO_TARGET_DIR | Cargo/scripts, external tool convention | Optional build-output relocation; not a cpuv runtime cache flag |
| GITHUB_REF_TYPE, GITHUB_REF_NAME | Release workflow | Tag/version consistency inputs from Actions |
| GH_TOKEN, GH_REPO, RELEASE_TAG | Release job shell | Publication credential/repository/tag; token comes from Actions secrets and is never documented as a literal |

## Self-management/script options

Native setup/self install: `--install-dir` optional path and `--no-path` boolean. Self update: `--version VERSION`, `--check`, `--force`, and `--release-base-url` with default `https://github.com/Bongusrihas/cpuv/releases`. Only stable three-part numeric versions are accepted by the Rust updater; the PowerShell bootstrap's version regex also permits a prerelease suffix. This difference is real, not a shared release-version parser.

PowerShell build parameters: DebugBuild, Install, InstallDir, NoPath, NoCompatCopy. Developer installer: BinaryDirectory (default repo target/release), InstallDir (default LocalAppData cpuv/bin), NoPath, NoCompatCopy. Packager: SkipBuild, DebugBuild, OutputDirectory (default dist). Bootstrap: Version, InstallDir, NoPath, ReleaseBaseUrl. Script parameters are local tooling interfaces, not keys stored in `cpuv.toml`.

Bootstrap tool schema: `[tools.NAME]` supplies version, URL, SHA-256, archive (`zip`/`tar.gz`), strip_components (generic default 1), executable, optional include/exclude relative lists. Current entries explicitly pin Conan 2.33.0, CMake 3.31.6, and Ninja 1.12.1. They are embedded at compile time, not user-overridden through the removed `--recipes` interface.

Sources: [CLI](../../crates/cpuv-cli/src/main.rs), [profile](../../crates/cpuv-cli/src/source.rs), [discovery](../../crates/cpuv-toolchain/src/msys.rs), [setup](../../crates/cpuv-cli/src/install.rs), [update](../../crates/cpuv-cli/src/update.rs), [bootstrap](../../install.ps1), [scripts](../../scripts/build.ps1), [tool manifest](../../build-tools.toml).
