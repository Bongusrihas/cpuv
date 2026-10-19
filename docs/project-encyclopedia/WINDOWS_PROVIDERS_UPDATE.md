# Windows provider update — 2026-10-19

Application version 0.0.3 adds MINGW64/MSVCRT binary packages alongside UCRT64/UCRT, native GCC management, and direct Conan recipe/remote operations. The original encyclopedia's 2026-10-17 symbol inventory, hashes, counts, and absence statements remain a historical baseline.

The toolchain crate exposes discovered pacman-capable roots, observed compiler inventories, saved compiler selection, and matching binary provider observations. The CLI toolchains module implements list/install/use/reset. Installation delegates explicit package requests to that MSYS2 host; package dependencies are upstream metadata. Compiler selection is verified by compilation and recorded without changing PATH. Nix/WSL is excluded from this Windows release.

Link planning now takes a provider namespace. Conan uses its own package names rather than synthesized MSYS2 names. A new environment preparation identity invalidates prior MSYS2 link plans. PE runtime policy checks both UCRT and MSVCRT directions, while COFF archive inspection now includes nested provider deployments; the baseline limitation about root-only archive inspection no longer describes this implementation.

The generic Conan context builder is shared by synchronization and native Conan commands. Compiler/profile identities and locks still separate provider homes. The default Conan profile records the selected compiler and saved flags. Full recipe references can be requested directly or declared in dependency tables. They force Conan for the whole graph, and upstream revision locks remain authoritative. cpuv adds no per-library recipe table or build patches.

Verification and development fixtures remain outside the source distribution. The Windows provider integration uses real MINGW64 package downloads and compilation plus a separate exported/created Conan recipe; fixtures are not product package special cases. See the current [provider guide](../windows-providers.md) and [release notes](../release-notes/v0.0.3.md) for commands, prerequisites, and compatibility limits.
