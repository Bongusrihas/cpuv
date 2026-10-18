# 17 · Glossary and conceptual foundations

[Index](MASTER_INDEX.md) · [Schemas](07_DATA_MODEL_AND_STORAGE.md) · [Architecture](03_ARCHITECTURE_DEEP_DIVE.md)

## Compiler and interface concepts

**ABI — application binary interface.** The binary agreement between compiled pieces: symbol names/calling conventions, object layout, exceptions, and standard-library configuration. Source compatibility does not guarantee ABI compatibility. cpuv observes parts of it in `AbiTuple` and compiler macros; see fingerprints below and `cpuv-toolchain/src/lib.rs`.

**ABI fingerprint.** A digest of recorded compatibility observations. cpuv hashes its serialized ABI tuple, while catalog cache keys include broader toolchain/profile inputs. Equal fingerprints do not prove equal unobserved upstream options. Related: profile, provenance, binary inspection.

**GCC driver and linker.** `g++` orchestrates preprocessing, compilation, assembly and linking; it invokes a toolchain-compatible linker rather than merely forwarding all work to any program named `ld`. cpuv records the driver and reports its linker query; driver options can select supported alternatives. Related: target triple, COFF.

**Target triple.** A compiler identifier describing a target environment, for example an architecture/vendor/OS combination. cpuv queries `-dumpmachine` and validates native Windows GCC targets; an x64 cpuv process does not make every target executable runnable on the host. See `probe_with_flags` and `doctor`.

**UCRT and MSVCRT.** Distinct Windows C runtime families whose libraries should not be interchanged casually. The managed binary provider is UCRT64-specific; catalog builds can retain a recognized native compiler's own runtime. Macro/configuration observations select the provider, not a hardcoded installation directory.

**POSIX thread model, SEH, SJLJ, DWARF2.** Compiler threading and exception-unwinding configurations, not interchangeable package labels. The UCRT64 provider predicate expects POSIX threads and SEH. Other configurations require compatible native builds and upstream support. See `parse_verbose`.

**libstdc++ dual ABI.** GCC's standard-library choice affecting interfaces such as `std::string`; `_GLIBCXX_USE_CXX11_ABI` is one observed flag. cpuv passes/guards compatible profile settings, but it is only one dimension of C++ compatibility. Related: debug iterators, profile.

**Language standard versus language ceiling.** The selected `-std=` mode controls this build; the ceiling is the newest mode accepted by compiler probes. Accepting a mode is not proof of complete standards conformance. `project.std = "default"` preserves the compiler's normal mode unless saved flags select another one.

**Compiler shim.** A small invocation layer supplying environment include/link/profile metadata to the real compiler. cpuv's CLI can dispatch as a shim when named `g++.exe`. The raw shim follows GCC output placement; the managed runner separately rewrites output locations. See `cpuv-shim::run` and `runner::run`.

**GCC specs.** GCC's internal rule customizations, optionally loaded with `-specs`. Generating a file does not make an unmodified compiler discover it. cpuv treats specs as experimental and rejects plans they cannot express; catalog profiles require the shim. See `generate_specs`.

**PE and COFF.** Portable Executable is the Windows executable/DLL format; COFF describes object-file structures found in object/archive inputs. cpuv uses goblin to inspect supported architecture/import/export evidence. Inspection is incomplete for nested catalog static archives and does not certify the entire ABI.

**DLL import/export and ordinal.** A compiled binary names required dynamic libraries and symbols, while a DLL exports available symbols. An ordinal refers to an entry number rather than a name. cpuv checks named edges when both DLLs are in its primary bin set; external and ordinal exports remain unproven.

**Static archive versus shared library.** Static archives supply object code to the final link; shared libraries supply dynamically loaded runtime code, with import libraries often used at link time on Windows. Public pkg-config metadata is not automatically a complete static/private link specification. Related: public Requires, runtime PATH.

## Package and metadata concepts

**Manifest versus lock.** A manifest expresses intended direct dependencies and configuration. A lock records a resolved graph/revision set and compatibility observations. Editing the manifest does not rewrite the environment immediately. MSYS2 and catalog locks use different schemas; see `Manifest`, `Lock`, `CatalogLock`.

**Direct root and transitive dependency.** A direct root is explicitly declared by the project; a transitive package is needed through another package. `rm` removes direct declarations and resolves reachability again. Removing one root does not remove a shared dependency still needed by another root.

**Snapshot resolver.** MSYS2 resolution selects from one rolling repository database rather than searching a historical universe of versions. cpuv's DFS checks constraints, virtual providers and conflicts. A comparison is not a promise that an older compatible version can be downloaded.

**Virtual provider.** A package's `provides` entry can satisfy a dependency named for an interface rather than that package. cpuv requires a valid unambiguous provider; silently choosing among multiple candidates is rejected. Related: snapshot, conflicts.

**ConanCenter and upstream recipe.** The community catalog supplies package metadata and executable build/resolution instructions. cpuv has no first-party per-library recipe registry, but using the shared catalog still executes upstream recipes. A package must exist and support the selected platform/compiler. See `source::sync`.

**Host and build context.** Provider graph roles distinguish libraries for the target application from tools used while building them. cpuv deploys host packages and generates their interfaces; build requirements can still execute on the machine. Current adapter profiles are native, not a general cross-compilation design.

**PkgConfigDeps.** The Conan generator producing `.pc` files from package/component interfaces. cpuv consumes its public metadata instead of inferring links from header names. Module names and aliases come from graph properties; they are not library-name branches in cpuv.

**pkg-config module, Cflags, Libs, Requires.** A module declares compile flags, public link flags, and dependency modules. cpuv implements a bounded reader with variable expansion, tokenization and version checks. It does not evaluate arbitrary shell text or CMake targets. Related: ownership, public/private interfaces.

**Link plan.** The selected public interface applied to compiler/linker invocation: includes, definitions, libraries, directories and flags. `link-plan.json` records authority and selected modules. It is independent of whether a source file happened to include one particular header.

## Storage, execution, and trust concepts

**CAS — content-addressed storage.** Data is located by a content digest rather than a project-specific name. cpuv's MSYS2 store uses BLAKE3 per-file blobs and archive-keyed package records. CAS identity does not authenticate the original publisher. Related: checksum, hardlink.

**SHA-256 and BLAKE3.** Hash functions mapping bytes to digests. SHA-256 verifies archives/tools/releases and deployed inventories; BLAKE3 addresses MSYS2 blobs and some identities. An attacker controlling both an archive and its advertised digest can still publish matching malicious content.

**Hardlink and private metadata replacement.** Multiple filesystem names can reference the same file content, so editing one hardlink can change other projects/cache content. cpuv breaks metadata hardlinks before relocation; payload is intended immutable. Copy fallback handles locations where hardlink creation fails.

**Relocation.** Rewriting installation-specific metadata to refer to the managed environment. MSYS2 prefixes and staged catalog `.pc`/`.cmake` paths are adjusted by provider-specific paths. This does not translate binary ABIs or make arbitrary native packages universally relocatable.

**Staging, commit, rollback.** Prepare a new generation privately, switch active files, and attempt restoration on failure. Several renames are not one atomic durable transaction. Backup-retention differences between backends are documented in chapter 19.

**Profile and package identity.** A profile records compiler/settings/flags used for dependency builds and consumers. cpuv separates catalog homes and contributes a profile identity to package IDs; upstream recipes can intentionally reduce identities. A cache key is not proof of hermeticity.

**Hermetic versus sandboxed.** A hermetic build controls all inputs; a sandbox restricts process access. cpuv provides neither complete property. A checksum-pinned tool or locked source still executes code with user permissions and can observe the surrounding environment.

**Offline mode.** First-party downloads are disabled and Conan remotes are disabled. Complete cached inputs can work. It is not a network firewall around arbitrary recipe/build subprocesses. Related: cache, trust boundary.

**Runtime PATH.** Search directories made available to compiler tools and Windows program DLL lookup. Managed execution prepends project/compiler paths instead of scattering DLLs in the source root. PATH is an executable trust input, not merely a convenient string.

**Hidden `.cpuv` state.** Local headers, libraries, metadata, build outputs and locks. Windows hidden attributes keep ordinary folder views tidy. Hidden does not mean encrypted, secret, or inaccessible to other permitted processes.

**Self update versus dependency removal.** `self update` replaces the invoked cpuv executable from trusted release assets. `rm`/`remove` changes a project's package roots and environment while retaining caches. No application `self uninstall` operation was found.

Primary implementation references: [toolchain](../../crates/cpuv-toolchain/src/lib.rs), [solver](../../crates/cpuv-solver/src/lib.rs), [store](../../crates/cpuv-store/src/lib.rs), [linking](../../crates/cpuv-relocate/src/linking.rs), [inspection](../../crates/cpuv-relocate/src/inspect.rs), [catalog](../../crates/cpuv-cli/src/source.rs), [runner](../../crates/cpuv-cli/src/runner.rs).
