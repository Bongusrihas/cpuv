# Application uninstall update · 2026-10-18

[Index](MASTER_INDEX.md) · [Current usage](../../README.md) · [Installation/release behavior](../releases.md)

This incremental feature supersedes the 2026-10-17 encyclopedia baseline's statements that application uninstall was absent. That baseline's source hashes, file counts, and checks remain a historical snapshot; they have not been represented as hashes of the new code.

## User interface and scope

```powershell
cpuv uninstall --dry-run
cpuv uninstall
```

`uninstall` removes the cpuv application. `rm`/`remove` still remove project dependencies. Application uninstall dispatches before project discovery or compiler probing, so it needs no project or working GCC. It uses the Windows self-deletion implementation in the already declared self-replace dependency and requires no user PowerShell command or Rust installation.

The plan starts at the invoked executable. When invoked from a standard per-user location, it can also remove byte-identical cpuv copies in the standard USERPROFILE/.local/bin and LOCALAPPDATA/cpuv/bin locations. A custom/portable invocation affects that installation only. Different binaries in those other locations are reported and preserved; it does not scan arbitrary PATH directories or execute other binaries to identify them.

Recognized sibling cpuv-shim.exe helpers must be regular PE files containing specific cpuv shim diagnostic markers. Legacy cpuv.cmd launchers require the cpuv-owned launcher comment. These are bounded local ownership heuristics, not cryptographic publisher authentication. Unrecognized files are preserved. No installation directory is recursively deleted, and no system/machine PATH entry is changed.

## PATH and preservation

Only dedicated installation directories are eligible for user PATH removal. If other files/tools remain, the directory stays on PATH. This preserves tools such as uv in shared .local/bin. Removal compares whole entries after native environment expansion and path normalization, preserves unrelated entries/order and the original REG_SZ/REG_EXPAND_SZ type, and broadcasts the environment change. Empty/relative PATH entries are not interpreted as the installation directory. Existing terminals retain their inherited PATH until reopened.

Project source, project manifests/locks, hidden .cpuv environments and shims, GCC, dependency caches and PATH backups are retained. Uninstalling the global CLI does not recursively traverse projects. No purge or self-uninstall alias was added beyond the top-level command.

## Implementation and failure paths

- [main.rs](../../crates/cpuv-cli/src/main.rs): `Action::Uninstall` and early `execute` dispatch.
- [uninstall.rs](../../crates/cpuv-cli/src/uninstall.rs): plan, ownership checks, preview, install/update locks, staged companions, recovery, and self deletion.
- [install.rs](../../crates/cpuv-cli/src/install.rs): native PATH normalization/removal and guarded PATH restoration.
- [CLI manifest](../../crates/cpuv-cli/Cargo.toml): adds the Windows environment API feature; no dependency-version change.

Dry run performs discovery/read operations only. Actual removal locks affected installations, renames companions into private staging directories, edits eligible user PATH entries, then schedules removal of the running executable. Before self deletion is scheduled, errors restore staged companions; restoration errors retain recovery directories. If self-deletion scheduling fails, PATH restoration checks that another process has not concurrently changed it before restoring the earlier value.

After scheduling, cleanup removes only known staged files and empty stage directories. Files held by other active processes may retain a clearly reported cleanup location. Successful scheduling is not a guarantee against every subsequent Windows process/filesystem failure. Multiple file and registry operations are not a crash-proof transaction. Different binaries or system-wide installations are outside automatic removal.

## Verification

Formatting, workspace Clippy with warnings denied, and the locked release build passed. Eight isolated checks passed: unchanged dry-run state; native deletion and recognized companion cleanup; expanded/quoted PATH removal while preserving registry type/order and empty/relative entries; shared-directory preservation; preservation of different executable/unrecognized launcher files; recovery from invalid PATH registry data; recovery from a Windows-locked helper; and execution of the real CLI uninstaller from a custom shared directory with developer tools absent from PATH. The grouped check record also confirms the real user's PATH remained unchanged throughout the verification.

Destructive tests used disposable installations. Production uninstall/PATH code was exercised by an external probe that redirected HKEY_CURRENT_USER inside the probe process to a disposable registry root, without adding a test override to the shipped command. The real CLI self-deletion check used a disposable shared directory that required no registry mutation. Project source, generated-environment sentinels, package-cache sentinels, unrelated tools, and rollback state were verified. All fixtures and reports remain outside this source checkout. The actual installed application is updated but is not uninstalled as part of feature development.
