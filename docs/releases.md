# Publishing a Windows release

Users install prebuilt executables. Rust, Cargo, Git, and the C++ build tools are required only on the developer machine or GitHub build runner.

## Build locally

From the source checkout in PowerShell:

~~~powershell
.\scripts\package.ps1
~~~

The package script creates these release assets under the ignored dist directory:

| Asset | Purpose |
| --- | --- |
| cpuv-setup.exe | Double-click native installer; no PowerShell or administrator rights |
| cpuv-setup.exe.sha256 | SHA-256 digest for the bootstrap download |
| cpuv-x86_64-pc-windows-msvc.zip | Portable executable, install.cmd, offline install.ps1, and license notices |
| cpuv-x86_64-pc-windows-msvc.zip.sha256 | Portable archive checksum |
| install.ps1 | Self-contained one-command download bootstrap |

The executable embeds the Rust code and statically links the Microsoft C runtime. A prebuilt installation does not require Rust, Cargo, Git, or a separate Visual C++ runtime installation. GCC is still required when a user compiles a C++ project.

## Publish on GitHub

Commit and upload the source first. Set the workspace version in Cargo.toml and update Cargo.lock and documentation together. Add release notes under docs/release-notes/vVERSION.md. Create and push a matching version tag, such as v0.0.1. The Release Windows executable workflow checks formatting, Clippy, and compilation, then packages the assets, verifies their checksum files, uploads all assets to a draft, and publishes the completed GitHub Release. A manual workflow run builds downloadable Actions artifacts without publishing a release.

Alternatively, create a GitHub Release for the matching tag and attach all five locally built assets above. Upload executable assets, not just GitHub's automatically generated source ZIP. When using manual upload, publish only after all assets are attached.

For the stable installer channel, clear "This is a pre-release" and select "Set as latest release". GitHub excludes prereleases from the latest release endpoint; if only a prerelease exists, releases/latest/download/install.ps1 returns 404 even when its assets are uploaded. The release workflow explicitly clears the prerelease flag before setting the completed release as latest.

Public installation then uses:

~~~powershell
irm https://github.com/Bongusrihas/cpuv/releases/latest/download/install.ps1 | iex
~~~

Equivalent Windows command with an explicit PowerShell process:

~~~console
powershell -ExecutionPolicy ByPass -c "irm https://github.com/Bongusrihas/cpuv/releases/latest/download/install.ps1 | iex"
~~~

Installed users can run cpuv self update, or cpuv self update --check to check availability. Publish a higher workspace version and matching tag for automatic upgrades. Same-version reinstallations require --force. The updater checks the release checksum and the downloaded executable's reported version before using Windows-safe self replacement. It updates the invoked executable without invoking the installer or changing PATH. Project environments remain intact. CPUV_UPDATE_BASE_URL or --release-base-url can select a trusted alternative release host with the same URL layout; HTTPS is required except for loopback test servers.

The direct cpuv-setup.exe download supports installation without a PowerShell command. Until a release with these assets is published, public install links will return a missing-release error. The installer does not fall back to compiling from source.

## Refresh an existing release manually

A push to main updates the source repository. It does not replace the assets of an existing release. Prefer a new patch version and matching tag when publishing changed executables so the tagged source and release binaries describe the same implementation.

For a same-version maintenance refresh, build from the intended clean source commit with scripts/package.ps1, record that commit in the release description, and replace all five assets listed above. In the GitHub release editor, remove the previous assets with those names and upload the rebuilt installer, ZIP, both checksum files, and install.ps1. Review the complete asset set before saving the release. Keep it stable and latest when it is intended for the standard installer channel.

Do not move a published tag just to replace assets. If the existing tag is retained, its automatic source archives still describe the original tagged commit; link the maintenance source commit explicitly in the release description. The rebuilt 0.0.3 maintenance assets implement the fixes in [9f6dcf6](https://github.com/Bongusrihas/cpuv/commit/9f6dcf6ab135d115e94603a5b525cc59ed242f7b).

An already installed copy with the same version will need:

```powershell
cpuv self update --force
```

Run this only after replacement assets are published. New installations use those assets through the normal setup or bootstrap download. The version string is unchanged, so it cannot distinguish the original and maintenance binaries by itself. See [troubleshooting](troubleshooting.md#check-the-executable-and-compiler-first).

## Installation behavior and trust

The native installer installs a single cpuv.exe in the current user's .local/bin, preserves unrelated user PATH entries and their expandable registry format, and broadcasts the environment change. Existing terminal processes need to be reopened. The PowerShell bootstrap also updates its own session PATH for immediate use. No machine-wide environment variables are changed.

For an isolated install use cpuv-setup.exe --install-dir DIRECTORY --no-path. Re-running setup replaces the executable; it preserves package caches and project environments. A running executable may prevent replacement, in which case close it and retry.

To remove an installed application, run cpuv uninstall; cpuv uninstall --dry-run previews the planned files and PATH changes. It removes the invoked executable and byte-identical copies in the standard per-user locations, plus recognized helpers. Custom/portable execution removes only that installation. Different executable copies are preserved and reported. User PATH entries are removed only for dedicated directories; shared .local/bin entries remain when other tools/files are present. No installation directory is recursively deleted. Projects, generated project environments, GCC, caches, and PATH backup files are retained. Existing terminals need reopening, and other active processes can delay cleanup of their old executable files. cpuv rm removes project dependencies and is separate from application uninstall.

Checksums detect disagreement between the downloaded executable and checksum asset. They do not authenticate an account or release that has been compromised. The executable is not Authenticode-signed in this release; Windows may display its normal downloaded-app reputation warning. Signing requires the maintainer's signing identity and is not simulated by the packaging workflow.
