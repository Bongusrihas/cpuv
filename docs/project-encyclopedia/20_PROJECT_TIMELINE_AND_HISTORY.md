# 20 · Timeline and recoverable history

[Index](MASTER_INDEX.md) · [Current status](16_IMPLEMENTATION_STATUS_AND_ROADMAP.md)

## Evidence scope

Local Git exposes three commits, the `main` branch and its local `origin/main` tracking ref, and tag `v0.0.1`. These were read with `git log`, `git branch -avv`, `git tag -n`, and `git show --stat`. No history rewrite, commit, tag creation, push, issue tracker access, or live release publication was performed for this audit. Tracking refs are local records, not proof of current remote state.

Dates below are the recorded commit dates in Asia/Calcutta-compatible +05:30 notation, not independently verified project creation or release publication times.

| Recorded date/time | Revision | Recoverable milestone |
| --- | --- | --- |
| 2026-10-17 01:25:13 +05:30 | `5c720bd` | Initial commit, containing the Apache license file. This does not establish an earlier implementation history. |
| 2026-10-17 12:56:47 +05:30 | `575683d` | Commit titled “Release cpuv 0.0.1 with portable GCC environments and source builds”; adds the seven crates, scripts, workflows, documentation, source registry, and lockfile in one large change. Tag `v0.0.1` is locally available. |
| 2026-10-17 13:29:30 +05:30 | `4889f1f` | Commit titled “Publish stable releases explicitly for latest installer downloads”; HEAD/main and local origin/main point here. Source/workflow evidence explicitly sets stable/latest publication. |

The large implementation commit does not reveal the internal order of individual feature development. No granular sequence of every resolver, linker, installer, or security fix can honestly be reconstructed from those three commits alone.

## Uncommitted working-tree evolution

The current manifest declares 0.0.2. Changes already present before this audit replace the embedded library recipe registry with the generic ConanCenter adapter, add pinned standalone tooling, add missing-root fallback and improved suggestions, preserve/migrate profiles, and update current documentation/version records. The old `recipes.toml` is deleted in the working tree; `build-tools.toml` and 0.0.2 release notes are new. This behavior belongs to the working tree, not automatically to `4889f1f` or the tagged 0.0.1 source.

The explicit application change in this request adds a visible `rm` alias to the existing remove command. Targeted checks and local installation/packaging completed. Encyclopedia generation then remained documentation-only, verified against a post-feature source baseline. No source version bump or separate release publication was performed for the alias.

## Documentation conflicts over time

The 0.0.1 release notes and historical changelog describe embedded fmt/Eigen/zlib recipes and custom registries. They are historical statements, not evidence those APIs remain current. The 0.0.2 working tree removes that library-specific source registry and `--recipes`. Current user-facing backend `conan` retains internal module/origin names containing `source` for compatibility and shared profile semantics.

The 0.0.1 stable/latest fix is backed by a commit; whether a remote release is currently downloadable is a different runtime/network fact and was not rechecked during this audit. Likewise the 0.0.2 source declaration and local `dist` assets do not establish a published 0.0.2 tag/release.

## Missing history

No local branch contains a separate older design/prototype series, detailed ADRs, issue-linked milestones, or recoverable historical benchmark record. Existing documentation discusses architecture corrections, but the original technical PDF and private development artifacts are not part of this source baseline. They are not treated as authoritative current architecture. Future maintainers should record schema/provider decisions and meaningful validations with each change rather than relying on conversation history.

Sources: [changelog](../../CHANGELOG.md), [0.0.1 notes](../release-notes/v0.0.1.md), [0.0.2 notes](../release-notes/v0.0.2.md), [release workflow](../../.github/workflows/release.yml), [workspace metadata](../../Cargo.toml), and the local revisions above.
