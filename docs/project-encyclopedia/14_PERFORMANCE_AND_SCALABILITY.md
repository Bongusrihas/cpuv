# 14 · Performance and scalability

[Index](MASTER_INDEX.md) · [Storage](07_DATA_MODEL_AND_STORAGE.md) · [Operations](15_ERROR_HANDLING_AND_OPERATIONS.md)

## Evidence boundary

No controlled cold/warm benchmark, memory profile, throughput dataset, or cross-machine latency distribution was found in the source checkout. The operation reports contain local diagnostics, not portable service-level objectives. This chapter derives characteristics from code; it does not revive earlier sub-millisecond or fixed-install-time targets as measured results. There is no database query workload or distributed service scaling topology.

## Algorithms and resource costs

Let P be snapshot packages, V/E the selected dependency vertices/edges, B total processed bytes, and F materialized files. Bounds below describe main operations, excluding operating-system and external-tool costs.

| Operation | Derivable behavior and consequence |
| --- | --- |
| Exact index lookup | BTreeMap lookup is logarithmic in P; graph traversal visits selected packages and constraints. |
| Virtual provider lookup | Scans the index's provides lists; worst-case repeated scans can dominate E lookups. The solver is not simply O(V+E) when virtual scans/comparisons are included. |
| DFS resolution | Selected/visiting sets avoid infinite cycle traversal; recursion depth follows dependency depth and is not separately bounded by a graph-depth option. Conflict checks may perform more provider scans. |
| Name suggestions | Filter names/query lengths to at most 256; edit distance is O(query length × candidate length) time with a single O(candidate length) row; ranked candidates use an ordered set and return at most five. |
| Download verification | O(B) streaming SHA-256; MSYS2 archives are not read wholly into memory. Reused archives also require full digest reads. |
| CAS validation/ingestion | O(B) hashing plus filesystem work for F entries. Warm reuse still validates cached payload, so it is not constant-time. |
| Materialization | Ordered case-folded ownership checks plus O(F) hardlink/copy operations; copies add payload byte cost. |
| Metadata parsing/traversal | pkg-config variable expansion makes at most 12 passes; graph traversal caps selected modules at 4,096. Repeated uniqueness checks on vectors may become quadratic in long flag lists. |
| Binary inspection | Reads inspected binary/archive files into memory individually; peak memory can track the largest inspected file plus parsed structures. Catalog nested archives currently skip one branch, a correctness gap rather than an optimization. |
| Catalog cache identity | Hashes the compiler executable and serialized observations/profile; changes can create separate provider homes and duplicate otherwise similar package work. |

Sources: [solver](../../crates/cpuv-solver/src/lib.rs), [fetch](../../crates/cpuv-fetch/src/lib.rs), [store](../../crates/cpuv-store/src/lib.rs), [linking](../../crates/cpuv-relocate/src/linking.rs), [inspection](../../crates/cpuv-relocate/src/inspect.rs), [catalog](../../crates/cpuv-cli/src/source.rs).

## Concurrency and backpressure

MSYS2 archive futures use `buffer_unordered` with default concurrency 50, configurable 1–256. Only available missing packages become transfers. Active/peak counters include a download task's retry lifetime; they are not a packet-level network utilization measurement. Extraction uses at most four blocking jobs independently of that network setting. Content-store per-package locks serialize duplicate ingestion. Project/build/catalog/update locks constrain mutations, but their scopes differ.

Catalog bootstrap tools prepare concurrently through `try_join!`. Conan receives the configured parallel binary-download value; source build scheduling follows upstream helpers. Compiler job count defaults to CPU available parallelism and can be limited. Upstream source dependencies may build in dependency order rather than all downloading/building simultaneously. Synchronous GCC/Conan calls can block runtime threads; this CLI has no separately scalable worker service.

## Memory, disk, and cache scaling

Compressed MSYS2 indexes are held in memory and may be cloned before blocking parsing. The compressed cap defaults to 64 MiB, with decompressed input bounded to sixteen times that cap and 1 MiB per desc entry. Parsed package strings and maps add overhead. These are upper input constraints, not measured peak RSS guarantees. Release updates are buffered in memory under a separate 128 MiB executable cap.

Hardlinks reduce same-volume duplication but require immutable payload discipline. Cross-volume materialization copies. Catalog full deployment copies package trees into project stages and checks provider manifests both before and after install; bootstrap inventories are also verified on reuse. Cache/profile/log directories have no automatic retention or garbage collection, so disk usage grows with archives, compiler/profile variants, and builds.

## Measurement and optimization plan

**Recommendations:** separately measure compiler probing, index fetch/parse, graph resolution, cache verification, extraction, materialization, metadata/inspection, native compilation, and program launch. Separate cold network installs, warm archive installs, warm verified environment reuse, and different provider paths. Record hardware, filesystem volume topology, antivirus, network conditions, versions, graph size, bytes, and repeat variability. Avoid comparing an empty environment to a large package installation.

Candidate optimizations include a provider-name index, validated metadata reuse inventories, shared transaction code, bounded-memory binary inspection, and measured extraction scheduling. Each changes integrity or concurrency assumptions and needs regression checks first. Neither raising concurrency nor skipping hashes is automatically an acceptable improvement. The existing `.cpuv/last-sync.json` provides phase diagnostics for MSYS2 and only package count/backend/total time for catalog mode.
