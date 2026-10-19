# Community catalog installation

cpuv uses compatible MSYS2 UCRT64 or MINGW64 packages first. A missing package, another native MinGW GCC profile, custom flags, or --catalog selects ConanCenter for the entire environment. There is no cpuv library recipe file to create.

For automatic fallback, a negative lookup from a cached MSYS2 index is refreshed once online before choosing ConanCenter. A newly downloaded index is not fetched a second time for the same operation, and offline mode never refreshes. Refresh errors are reported as unconfirmed availability rather than absence. cpuv lists the missing requirements; if fallback fails, its error retains the MSYS2 lookup and similar names along with the provider log. Suggestions come from that repository and do not select or alias a package. Published names and versions can differ between providers; package existence in another ecosystem is not proof of MSYS2 or ConanCenter availability. Catalog name/version resolution occurs before compilation, so a resolution failure alone is not evidence of ABI or linker incompatibility.

```powershell
cpuv init
cpuv add xtensor
cpuv run g++ program.cpp
```

ConanCenter supplies package versions, transitive dependencies, build instructions, and public compiler/linker metadata. cpuv contains a generic adapter and bootstrap tool manifest. Upstream recipes still exist because libraries have different build systems; arbitrary unregistered archives cannot be built automatically.

## Provider selection

```powershell
cpuv --catalog add fmt zlib
cpuv sync
cpuv sync --offline
```

The project records backend = "conan" after success and retains that provider. Names must match that catalog exactly. cpuv does not translate ecosystem names or combine MSYS2 and Conan binary graphs. Failures report upstream diagnostics and a persistent log path.

Compatible packages are reused; missing binaries build with the selected native GCC and its matching gcc. --from-source forces source builds, including build requirements, and may need extra tools and considerable time. Packages must exist in the catalog and support the compiler, features, and platform. Cygwin and cross-OS catalog builds are outside this adapter.

## Compiler and profile

Choose the compiler with --compiler, CPUV_CXX, CXX, or PATH. GCC selects its own linker. Catalog projects start with project.std = "default". The observed C++ mode, architecture, compiler version, libstdc++ ABI, executable identity, and flags define the profile. Unsupported upstream language requirements fail explicitly.

CPPFLAGS applies to C and C++; CFLAGS applies to C; CXXFLAGS applies to C++; LDFLAGS applies to linking. Windows quoting rules apply. Flags are saved and passed to dependency builds and application compilation:

```powershell
$env:CXXFLAGS = '-O2 -D_GLIBCXX_USE_CXX11_ABI=0'
cpuv sync
cpuv run g++ program.cpp
```

Unset variables retain locked values. Remove the profile variables and run cpuv sync --reset-profile to clear them. Run sync after changing settings. Common conflicting consumer ABI overrides are rejected. Matching inputs are compatibility evidence, not proof of every interface. The adapter clears implicit CMake Release C/C++ flags; upstream target-specific options remain controlled by upstream metadata.

## Tooling and transfers

build-tools.toml pins official standalone Conan, CMake, and Ninja archives with SHA-256 verification. No system Python, Rust, Git, CMake, or Ninja is required for bootstrap. Particular upstream builds may need other tools. Automatic bootstrap targets x64 Windows.

--concurrent-downloads configures cpuv concurrency and Conan parallel binary transfers. --build-jobs controls supported parallel build helpers. Actual parallelism depends on the graph and upstream metadata.

cpuv byte, extraction, and timeout limits cover MSYS2 and bootstrap archives only. Conan owns its package/source downloads and extraction; cpuv does not wrap arbitrary recipe network calls. The separate 64 MiB index cap does not limit package archives.

## Locks, cache, and deployment

Version-3 cpuv.lock embeds upstream revision references, resolved package versions, observations, and portable flags. Local executable/cache paths stay in .cpuv and the shared cache. Commit cpuv.toml and cpuv.lock; exclude .cpuv. Legacy source manifests and version-2 locks migrate during successful synchronization, resolving catalog metadata anew.

Profile identities separate Conan homes and participate in package IDs. Upstream header-only/tool recipes may intentionally reduce identities. cpuv checks Conan cache manifests before and after installation, deploys host packages privately, relocates generated paths, collects DLLs with collision checks, and inspects binaries before committing. Failure preserves the active environment and configuration; failed restoration preserves recovery backups.

Packages and their upstream-packaged licenses stay in .cpuv/env/full_deploy. Application outputs stay in .cpuv/build. Logs stay in the cache's catalog/logs directory. link-plan.json records public modules; ambiguous interfaces require explicit [link] configuration.

Offline mode disables Conan remotes. Complete cached binaries can be redeployed without network. Missing inputs fail; recipe subprocesses are not sandboxed and may independently fetch sources if rebuilding.

Conan recipes execute Python code with your permissions, including binary resolution. Builds execute further upstream code. Checksums identify content; they do not certify safety or a compromised publisher. See [security](../SECURITY.md) and [compatibility](compatibility.md).

## Upstream documentation

- [Standalone Conan installation](https://docs.conan.io/2/installation.html)
- [Conan install](https://docs.conan.io/2/reference/commands/install.html)
- [PkgConfigDeps interfaces](https://docs.conan.io/2/reference/tools/gnu/pkgconfigdeps.html)
- [Deployers](https://docs.conan.io/2/reference/extensions/deployers.html)

## Additional Conan remotes and recipes

cpuv conan exposes upstream remote, export, create, profile, and cache commands in the same managed compiler/profile home used by synchronization. Default profiles select that GCC and saved flags. Full recipe references (name/version@user/channel, optionally #revision) can be passed to cpuv add or declared with a reference field in a dependency table. They select Conan for the whole graph; distribute private/local recipes separately from cpuv.lock. See [Windows providers and toolchains](windows-providers.md) for commands and trust boundaries.
