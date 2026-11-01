# Phase 3: compatibility, release trust, and maintenance (unreleased)

Phase 3 extends the finite Phase 1/2 acceptance workflows. It does not claim universal compiler or library support. The published 0.0.4 assets do not include these source changes.

## Reproducible compatibility gate

```powershell
$compilers=@(./scripts/select-test-compilers.ps1 -IncludeX86)
./scripts/test-phase3.ps1 -Compiler $compilers -EvidenceDirectory ./tmp/phase3-evidence
```

The selector uses the numeric versions of installed Microsoft toolset directories. It chooses the oldest and newest available compiler per target architecture, removing duplicates when only one is installed. On x64 hosts the optional x86 lane runs natively under Windows emulation. On ARM64 it selects ARM64 targets and prefers ARM64-hosted tools. An installed single version is evidence for one version, never a historical matrix by itself. Explicit GCC/GNU Clang/clang-cl paths can also be passed to the gate.

The retained consumer fixture builds a project-owned checksummed source archive through the generic CMake adapter. It checks:

- A C static component, a C shared component, a header-only interface, and their ordered aggregate target.
- C and C++ consumers, build-time generated headers, and CTest execution.
- A window-subsystem application which exits without opening a window, with a real resource compiler input. The test verifies its PE subsystem and resource directory before executing it.
- An unavailable provider request, preservation of the installed request/lock/environment after rejection, and subsequent successful consumer builds.
- Frozen offline restoration with an HTTP request counter proving no fixture-provider traffic.

Lifecycle tests copy the executable into an isolated installation, leave the real user's PATH unchanged, and serve releases from a loopback server. Missing releases, wrong checksums and wrong executable versions must preserve the installation. On x64 the same-version forced update exercises native executable replacement. Other CPUv architectures must explicitly reject the current x64-only update service. Dry-run and real uninstall must preserve unrelated files and unowned launchers. The ordinary workspace suite also runs these lifecycle tests.

Phase 2 retains the broader source/SDK, ABI guard, profile, generation, runtime plugin/delay imports, fresh-cache transfers, modified-transfer rejection and MSYS2/vcpkg corruption-recovery scenarios. The new gate complements those checks. Compiler-tree relocation and every missing provider source/tool combination are not established by the new fixture.

## Machines and evidence

The Windows workflow configures Phase 3 jobs on Server 2022, Server 2025, and the native `windows-11-arm` runner. Each job retains source hashes, OS/process architecture, actual compiler observations, logs, and measurements. ARM64 native execution is claimed only after that job actually passes. The compiler and library versions in the evidence define the tested boundary.

`desktop-compatibility.yml` is a manual lab workflow for desktop Windows 10/11. Provision an isolated X64 runner with Rust, Git, PowerShell 7, Microsoft C++ Build Tools/SDK, network access for the verified bootstrap tools, and labels `cpuv-compatibility-lab` plus `cpuv-windows10` or `cpuv-windows11`. Reset the machine between runs. This workflow does not run pull requests on self-hosted machines. Merely adding it does not establish a desktop-image pass; preserve the exact image/build and evidence after execution.

GitHub documents [available hosted runner labels](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job). Different installed toolsets and images can expose further issues; retained failed logs are part of the diagnosis.

## Dependency and bootstrap inventory

```powershell
cargo fetch --locked --target x86_64-pc-windows-msvc
python scripts/inventory.py tmp/dependencies.cdx.json
cargo install cargo-audit --version 0.22.2 --locked --no-default-features
cargo audit --file Cargo.lock --json
```

The inventory requires Python 3.11 or newer, uses Cargo's resolved Windows x64 dependency graph and the standard TOML parser, and emits CycloneDX 1.6 JSON. It records declared crate licenses and available registry SHA-256 checksums, dependency edges, the lockfile digest, and bootstrap tool versions/download URLs/hashes/licenses. It excludes private local paths and credentials. The scope is the application build/test graph and pinned bootstrap applications, not an exact binary reachability report or a complete inventory of each bootstrap executable's embedded dependencies. User libraries retain their own provider/source receipts and licensing requirements. Missing or ambiguous license/provenance entries fail inventory generation for review.

The security workflow checks `Cargo.lock` on pushes, pull requests, manual runs and weekly schedules, retaining the RustSec JSON result. A failed advisory-database fetch or registry check is a failed audit, not a clean verdict. Review future advisories and license changes; do not add blanket advisory ignores to make the gate green.

The initial audit found certificate trust advisories in `rustls-webpki`/`webpki-roots`, ZIP extraction advisories, and Zstandard context/decompression advisories. Update to patched dependency releases and retain the lockfile. The ZIP advisories concern APIs CPUv does not call; CPUv still keeps its own bounded extraction and path/link checks. Updating the dependency avoids carrying known vulnerable code into future usage. The unmaintained progress-formatting dependency is removed by updating its parent progress library.

The native GUI/resource fixture exposed a separate bootstrap packaging bug: CPUv's CMake extraction filter omitted `cmcldeps.exe`. Ninja's Microsoft resource rule consequently invoked `RC` with helper arguments that the resource compiler could not understand (`RC1107`). Include the helper from the same verified CMake archive; no library or project-name exception is needed. The fixture retains both a real `.rc` compilation and the resulting PE resource-directory assertion.

## Release trust and build isolation

Release CI requires optimized Phase 1, Phase 2 and Phase 3 gates, the dependency/license inventory, and a successful current dependency audit before uploading assets for publication. It emits GitHub build-provenance attestations for the setup executable, portable ZIP and inventory. A new release run is required to establish this evidence; existing 0.0.4 assets are unaffected.

Verify a downloaded artifact before running it:

```powershell
gh attestation verify ./cpuv-setup.exe --repo Bongusrihas/cpuv
```

The command requires GitHub CLI/network access and verifies the repository/workflow provenance; compare its verified commit and workflow identity with the intended release. See [GitHub artifact attestation verification](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations). Attestations are separate from Windows Authenticode publisher signing and do not certify upstream recipes. The in-application updater currently checks HTTPS, release checksums and executable versions; it does not automatically enforce these attestations or a pinned publisher signing key.

Native builds still execute with the user's permissions. Job Object process/memory/deadline limits bound resource use and cancellation; they do not isolate files, registry access or network traffic. For untrusted recipes, use a disposable VM/account with no personal credentials or writable shared folders. Export only verified outputs and receipts. Automatic per-build VM isolation, signed upstream-package enforcement, Authenticode key provisioning and an in-updater attestation policy remain explicit design work; no weaker checksum claim substitutes for them.

## Measurement policy

The native gate records per-operation wall times, including initial resolution/build, consumer tests, repeated verification and offline sync. The evidence summary includes sample counts, median and nearest-rank p95. These are small integration samples, not statistically stable benchmarks. Storage, virus scanning, compiler/SDK size and concurrent work affect results. Initial bootstrap and warm-cache operations must be compared separately. No absolute time threshold is used to turn normal runner variation into a compatibility failure.

Use repeated measurements on the same image/toolset to identify expensive probing, hashing, network or build work before optimizing. Keep content verification and transaction guarantees. Historical compiler releases absent from the matrix, desktop images without lab evidence, broader GUI frameworks and all library/version combinations remain ongoing coverage work.
