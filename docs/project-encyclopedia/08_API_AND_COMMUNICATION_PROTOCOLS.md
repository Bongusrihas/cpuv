# 08 · APIs and communication protocols

[Index](MASTER_INDEX.md) · [Configuration](12_CONFIGURATION_AND_ENVIRONMENT.md) · [Walkthrough](06_CODE_WALKTHROUGH.md)

## Interface boundary

The public API is a command line plus project/lock/runtime files. There is no first-party HTTP endpoint, RPC server, socket listener, event bus, application session, or database API. Rust crates communicate in-process through functions/types. Compiler/provider/build-tool integration uses OS process creation, argument vectors, environment, filesystem outputs, and exit status.

## Command contract

| Command | Input and side effects | Output/errors |
| --- | --- | --- |
| `init` | Optional global compiler/project options; creates manifest and local state; rejects existing manifest | Initialization diagnostics; compiler or filesystem failure exits 1 |
| `add [--needed] PACKAGES...` | Required roots with optional comparison; updates dependency graph; explicit host tooling may invoke pacman | Human progress/package list; preserves satisfied locks with `--needed` where checks apply |
| `rm PACKAGES...` / `remove PACKAGES...` | Required exact direct keys; rebuilds remaining graph and prunes unreachable packages | Shared cache retained; non-direct names fail before commit |
| `sync` | Reads project/lock; replays MSYS2 lock or synchronizes catalog revisions | Backend progress and local last-sync report |
| `lock` | Intentional re-resolution/update operation | Replaces graph after successful preparation; rolling repos may no longer contain old versions |
| `run [--no-run] COMMAND...` | Compiler tokens or another project command; `--` divides compiler and application arguments | Child streams and exit status; managed compiler artifacts under `.cpuv/build` |
| `clean` | Scans loose root artifact extensions | Archives preserved files; not cache deletion |
| `shell [--print]` | Reads generated activation | Prints script or starts activated PowerShell |
| `toolchain` | Global compiler selection | Pretty JSON `Toolchain` observations |
| `doctor [-- FLAGS...]` | Real compiler and optional driver flags | Pretty JSON compile/link/run evidence or contextual error |
| `info` | Manifest/lock, project/cache | Human package list and location information |
| `self install` | Optional install directory, no-path | Copies executable; optionally changes user PATH |
| `self update` | Version/check/force/release-base options; rejects offline | Availability or update message; downloads and verifies before replacement |

Clap handles syntax/help and returns a usage error for missing required package arguments. Rust application errors use contextual stderr and status 1. Child commands/programs return their own status, with unavailable status represented as 1. There is no typed JSON error protocol or globally stable machine-readable progress format. `toolchain` and `doctor` expose JSON, while ordinary command output is human-oriented.

Example observed removal semantics:

```powershell
cpuv rm fmt
cpuv rm fmt zlib
cpuv rm --offline fmt
```

Multiple removals are all validated against the local manifest copy before saving. Repeating a removed root is an error, not an idempotent success. Generated environment preparation is transactional only within the documented filesystem limitations.

## HTTP clients and external services

| Source → destination | Transport/request | Response and validation | Retry/deadline |
| --- | --- | --- | --- |
| Fetcher → selected MSYS2 mirror | HTTPS GET `/<database>`, default `ucrt64.db` | gzip/zstd tar records; namespace, filename, checksum and metadata-size validation | Up to 3 attempts; 15s connect, 60s idle read, configurable whole-transfer timeout |
| Fetcher → selected mirror | HTTPS GET `/<indexed filename>` | Exact indexed length and SHA-256; streamed temporary file | Up to 3 attempts; same client policy |
| Tool bootstrap → pinned upstream URLs | HTTPS GET ZIP/tar.gz | Pinned SHA-256, size/extraction limits, executable presence and inventory | Up to 3 attempts; same shared client policy |
| Updater → release host | GET `/latest`, `/download/vVERSION/cpuv-setup.exe` and `.sha256` | Versioned tag redirect; checksum syntax, digest, MZ signature, candidate reported version | Asset retry up to 3; 15s connect, 180s request timeout; separate 4 KiB checksum / 128 MiB executable caps |
| PowerShell bootstrap → release host | Invoke-WebRequest for setup/checksum | SHA-256 then native setup execution | Platform HTTP behavior; does not inherit Rust transfer limits |
| Conan → catalog/source hosts | External provider-controlled requests | Provider schema/cache/recipe policies | Provider-specific; no first-party universal transfer deadline |

Loopback HTTP is permitted for local fixtures and trusted local services. cpuv is a client: it does not allocate a listening port. No application-specific HTTP credentials are declared for these clients; platform/network configuration and provider behavior are external concerns. No live endpoint availability was assumed from source inspection.

## Process and file contracts

GCC queries use `-dumpmachine`, verbose preprocessing/macros, syntax-only language probes, and `-print-prog-name=ld`. Compilation uses the recorded executable with metadata-supplied arguments; user compiler spellings do not freely choose another binary at run time. Program launch receives project CWD, runtime PATH, and CPUV_ENV. The raw shim rejects `@response` files and resolves metadata adjacent to its executable or by ancestor search; it does not use CPUV_ENV as its first lookup input.

Conan receives explicit profiles, `--requires`, lock paths, deployment/generator options, parallel-download configuration, and `CONAN_HOME`. Its graph JSON must contain `graph.nodes`; host nodes supply references, package folders, and `cpp_info`. PkgConfigDeps produces `.pc` interface files read by `linking::apply`. This is a version-sensitive external contract, not a formally versioned cpuv RPC schema. Pacman integration is restricted to querying/installing explicitly requested `base-devel` in the selected MSYS2 host.

Sources: [CLI](../../crates/cpuv-cli/src/main.rs), [fetch](../../crates/cpuv-fetch/src/lib.rs), [policy](../../crates/cpuv-fetch/src/policy.rs), [catalog](../../crates/cpuv-cli/src/source.rs), [update](../../crates/cpuv-cli/src/update.rs), [bootstrap](../../install.ps1), [host tools](../../crates/cpuv-cli/src/host_tools.rs).
