# cpuv technical architecture and engineering specification

**Software version:** 0.0.2. **Specification revision:** 1.3, shared community catalog.

**Author:** Bongu srihas. **Co-authors:** Gemini 4 argon and GPT-6 sol.

This document describes the implemented version 0.0.2 architecture, its supported compatibility boundary, and the responsibilities of each component. All first-party runtime implementation is included in this source distribution.

## 1. Supported boundary

The managed binary ecosystem is MSYS2 UCRT64 on Windows x64: GNU GCC, UCRT, POSIX threads, SEH, and libstdc++ dual ABI 1. A compiler is discovered from an explicit selection, CPUV_CXX, CXX, or PATH. There is no personal installation path in the source. Other supported native MinGW GCC profiles use their own compiler/runtime through the ConanCenter adapter. Missing MSYS2 packages use the same automatic fallback.

vcpkg, unregistered GitHub C++ archives, MINGW64 binary packages, MSVC/Clang integration, Cygwin catalog builds, and cross-platform binary relocation are deferred. MSYS2 resolution uses one rolling repository snapshot. ConanCenter resolution uses its upstream dependency metadata and revision lock.

The standalone cpuv executable embeds its Rust code and statically links its Microsoft C runtime. Users need no Rust, Cargo, Git, or PowerShell to use native setup. They still need GCC to compile C++ programs; cpuv does not bundle or install GCC.

## 2. Dependency and linking authority

The package database and cpuv.lock define the resolved package graph, checksums, versions, and ownership. Selected package-owned pkg-config modules define public compiler/linker requirements through Cflags, Libs, and Requires, including version checks and transitive modules. This graph can include system libraries that the application's source never directly includes.

Source include scanning has no authority over library selection. The header inventory is diagnostic only. Generated headers, conditional includes, and object-only links must produce the same package link plan. Installed archives are not linked merely because they exist.

Automatic module selection uses package ownership: a matching package/module name or the direct package's single exported module. There are no named-library defaults, aliases, sibling-library injection, or special include directories. Ambiguous or missing compiled-library exports fail with instructions to configure the project explicitly. For example:

~~~toml
[link]
pkg_config = ["Qt6Widgets", "fmt"]
libraries = ["ws2_32"]
flags = ["-pthread"]
~~~

Explicit selection replaces automatic defaults. The generated link-plan.json records modules, libraries, and source authority. The bounded pkg-config reader supports variable expansion, quoted tokens, continuation lines, public Requires constraints, and selected public flags. It rejects unresolved variables, missing modules, and managed paths outside the environment. It does not evaluate arbitrary CMake targets or enable static/private interfaces. Unsupported metadata requires explicit configuration rather than guessing.

## 3. ABI is a filter, with additional evidence

An ABI fingerprint is a coarse compatibility filter. Equal hashes do not prove equal object layouts, instruction requirements, packing, feature macros, debug iterators, compiler ABI options, or exported interfaces. Header-only and C interfaces also have constraints; neither is automatically universally compatible.

cpuv records GCC target/version, architecture, C runtime, thread and exception models, libstdc++ release/dual ABI, language ceiling, linker selection, and additional predefined ABI macros. Project dependency checks also compare selected runtime package versions. These observations narrow candidates; unknown upstream build options remain unknown.

Before committing a new managed environment, binary inspection checks PE architecture, direct runtime imports, and recognized COFF archive member architectures. It inventories exports/imports and checks named imported symbols when both DLLs are in the managed bin directory. A missing packaged export or machine mismatch aborts the staged update. compatibility.json records this evidence and explicitly sets binary_compatibility_guaranteed to false.

External DLL exports, ordinal imports, CPU ISA requirements, all header feature flags, packing, RTTI/exception behavior, and LTO compatibility are not proved by this inspection. Common incompatible user options such as -m32, altered packing, libstdc++ debug mode, or dual ABI 0 are rejected when managed binaries are present. This is a bounded guard, not a complete ABI-option theorem.

Binary inspection does not certify every C++ interface. Source mode builds libraries with the selected compiler and saved profile, then applies architecture and packaged-symbol inspection without imposing UCRT on an MSVCRT compiler. It rejects common conflicting consumer ABI flags. Upstream target-specific flags and ABI-sensitive interfaces still require compatible recipes and validation.

## 4. Compiler and linker integration

The compiler shim is the primary integration. cpuv run g++ invokes the recorded real GCC with the prepared metadata link plan and places outputs under .cpuv/build. An activated shell puts .cpuv/bin/g++.exe before the real compiler on PATH; the same executable dispatches as a shim under that name. It does not recursively select itself as the compiler. Catalog projects preserve the compiler's language default unless explicitly configured; unsupported upstream language requirements fail rather than changing the compiler silently. cpuv doctor performs a real compile/link/run check; discovery alone is not proof of a usable installation.

GCC performs preprocessing, compilation, assembly, and linker invocation. GCC's own toolchain configuration chooses its linker; compatible driver options are forwarded. cpuv does not replace ld with an arbitrary Microsoft linker or translate incompatible object formats. Program execution receives the dependency DLL directories through PATH and keeps the project working directory.

Generated GCC specs are optional and experimental. A real compiler must explicitly load them with -specs=.cpuv/env/cpuv.specs, with CPUV_ENV pointing to the environment. Merely writing the file does not make plain g++ discover it. The supported subset handles public library groups, including revisiting libstdc++ for static archive references. Plans requiring unsupported extra specs flags fail and direct the user to the shim.

Source profiles require the shim. They do not inject GNU linker group options automatically; the selected driver and package metadata determine the link invocation.

## 5. Storage, transactions, and trust

Downloads are bounded by a configurable concurrency limit, default 50, and verified against SHA-256 values from the selected package index. Archives stream to disk, with a default 16 GiB per download, 64 GiB expanded payload, and one million entries per archive. Limits and the default one-hour transfer deadline are configurable. The compressed index has a separate 64 MiB memory cap; decompressed index input is bounded to sixteen times that cap. These metadata limits do not restrict package sizes. Archives use temporary files, retries, safe extraction, and independently bounded extraction work. BLAKE3 blobs and archive-keyed package records support hardlink materialization with a copy fallback.

Paths, unsafe links, device names, collisions, and cross-prefix payload are validated. Known host pacman metadata under usr/share/libalpm/hooks and scripts is counted against extraction limits and discarded; project installation never executes these hooks. An explicitly requested base-devel host transaction uses pacman in the selected MSYS2 installation and has pacman's normal script behavior. Host state is separate from project ABI payload and cannot be rolled back with a project environment swap.

Relocated metadata is privately copied before editing shared content. Project/package locks coordinate operations. A staged environment is committed with rollback on normal errors; this is not a crash-proof transaction against all filesystem or power failures. Cached hardlinked files must be treated as immutable.

Checksums verify agreement with the index, not the authenticity of a compromised repository. Package signature verification and a complete software supply-chain trust policy remain outside version 0.0.2.

### Shared community catalog pipeline

The CLI contains a generic Conan adapter rather than a source recipe registry. build-tools.toml pins standalone Conan, CMake, and Ninja archives and checksums only. No named C++ library recipes or patches are embedded in cpuv.

A missing MSYS2 root, a different native GCC profile, custom flags, or --catalog selects ConanCenter for the entire dependency environment. cpuv never combines an MSYS2 binary graph with a Conan graph. The legacy backend = "source" spelling is accepted for migration and becomes "conan" after successful synchronization.

The adapter translates the observed GCC target, language mode, libstdc++ ABI, compiler paths, and saved flags into host/build profiles. A compiler/profile identity participates in Conan package IDs and separates local Conan homes. Upstream recipes can intentionally reduce identities for header-only or build-tool packages. Binary compatibility remains bounded by upstream metadata and the actual toolchain.

Conan resolves the transitive graph and builds missing binaries. --from-source forces all graph builds, including build requirements. Upstream recipes select their own build systems, sources, tool requirements, patches, and license installation; cpuv does not author these per library. A version-3 cpuv.lock embeds Conan's revision lock, resolved package versions, portable flags, and compiler observations. Local executable/cache paths stay in local diagnostics.

cpuv verifies the Conan cache manifests before and after installation. full_deploy copies host packages into a private staged environment; PkgConfigDeps supplies their public interfaces and transitive requirements. Package/component names and aliases are read from graph metadata. Generated paths are relocated, DLLs are collected with collision checks, PE/COFF inspection runs, and a SHA-256 deployment inventory is written before transactional commit. Failure preserves the active environment and project files; recovery backups are retained if rollback itself fails.

--concurrent-downloads configures Conan's parallel binary transfers. Its package/source downloader, extraction, and recipe subprocesses have their own policies; cpuv's byte/extraction/time limits apply to MSYS2 and bootstrap archives only. --build-jobs controls supported upstream build helpers. Offline installation disables Conan remotes; a complete binary cache can be redeployed without network. Upstream recipe subprocesses are not sandboxed and may independently attempt network access if rebuilding.

The adapter is not a universal build-system inference engine. Packages must exist in ConanCenter and support the selected GCC, features, tools, and platform. See [catalog installation details](source-builds.md).

## 6. Operational timing

Download concurrency is configurable, with a default transfer limit of 50. Actual parallelism depends on the resolved graph and available transfers. CPU and disk work, including extraction and content validation, have independent costs.

The MSYS2 path writes .cpuv/last-sync.json with compiler probing, index fetch/parse, dependency resolution, cache validation, extraction, materialization, metadata preparation, binary inspection, and environment timings. Catalog mode records its backend, package count, and overall synchronization time; detailed provider operations remain in its build log. These machine-local diagnostics describe an operation; they are not a portable latency guarantee. Cache reuse still validates package contents and required files.

Package size, file count, network, disk, antivirus, and compiler startup affect performance. This release promises neither a fixed installation time nor performance equivalence with uv. No project-specific benchmark reports are included in this source distribution.

## 7. User workflow after downloading cpuv

1. Install the prebuilt cpuv executable. It installs cpuv, not GCC or a new ABI.
2. cpuv init probes the installed or selected g++, records the local toolchain, and prepares the shim. Native compiler discovery remains machine-independent.
3. cpuv add selects compatible UCRT64 binaries or resolves catalog dependencies and builds missing binaries with the selected native compiler/profile. Both paths verify inputs, prepare the package-metadata link plan, and inspect outputs before commit.
4. cpuv run g++ main.cpp supplies the configured include/link requirements to that GCC. GCC invokes its own assembler/linker, and cpuv runs the resulting executable with the project DLL paths.
5. Change saved flags and run cpuv sync to rebuild source dependencies for that profile. Absent catalog entries or unsupported compiler features fail explicitly. cpuv cannot change an already compiled library's ABI by changing a hash or appending flags.

## 8. Source and extension boundary

The source tree contains the CLI, generic catalog adapter and pinned tool bootstrap, toolchain probe, resolver, fetch pipeline, content store, relocation/linking/inspection code, compiler shim, native installer, self updater, and application uninstaller. Cargo manifests and Cargo.lock describe third-party dependencies. Build and packaging scripts are included; test suites, development fixtures, generated PDFs, and recorded benchmarks are excluded.

cpuv rm (also remove) edits direct project requirements and synchronizes the remaining dependency graph. cpuv uninstall operates independently of project initialization or compiler discovery: it identifies the invoked installation and recognized helpers, stages helper removal, edits eligible user PATH entries, and schedules native Windows executable deletion. Preparation failures restore staged files and attempt conditional PATH rollback. Shared tool directories, other binaries, project environments, and caches are preserved. See the [uninstall implementation update](project-encyclopedia/APPLICATION_UNINSTALL_UPDATE.md).

Windows CI checks formatting, Clippy, and compilation. The release workflow creates the prebuilt assets. Adding a registry or compiler family requires an explicit metadata/layout/profile adapter and compatibility validation before activation. Complete build provenance, signature verification, and universal ABI conversion are outside this release. See the [compatibility policy](compatibility.md).

## References

- GCC ABI policy: https://gcc.gnu.org/onlinedocs/libstdc++/manual/abi.html
- GCC specs: https://gcc.gnu.org/onlinedocs/gccint/Spec-Files.html
- pkg-config: https://www.freedesktop.org/wiki/Software/pkgconfig/
- MSYS2 environments: https://www.msys2.org/docs/environments/
