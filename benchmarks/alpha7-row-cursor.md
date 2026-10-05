# Canonical sparse row boundary search

The preceding packed block now resolves its requested start/end by binary search
instead of scanning unrelated cells. No index, allocation or shadow model is
added. See [ADR 0068](../docs/decisions/0068-binary-sparse-row-boundaries.md).

## Method and scope

Identical original public-core probe source, Rust 1.99 release flags/dependencies,
before core `a1c11a9` (core unchanged since `9b5311b`) and the candidate. Build
the core-only release library, then compile `benchmarks/row_cursor.rs` with
`rustc --edition=2024 -C opt-level=3 -C lto=thin -C codegen-units=1`, supplying
the matching library through `--extern crabxl_core` and `-L dependency`.

One warmup and three rotating serial samples. Each worker creates one canonical
integer model, then traverses it five times. Dense coordinates cover ten columns;
sparse coordinates skip alternate rows/columns and include empty gap queries.
Every returned value/sparse coordinate and total count/checksum are checked.
No XLSX, binding, external engine or worksheet clone is involved. Full wait4
wall/CPU/RSS includes creation/destruction; the separate inner traversal timer
excludes generation but includes verification. Builds/tests/profiling do not
overlap timing. Hashes and cleanup/temporary-byte evidence are retained.

## One million retained cells, five traversals

Median inner traversal seconds / full process seconds:

| Shape and API | Prior | Binary boundaries |
| --- | --- | --- |
| Dense row iterator | 0.1969 / 0.2353 | 0.1374 / 0.1760 |
| Sparse row iterator with gap rows | 0.3075 / 0.3548 | 0.1795 / 0.2159 |
| Dense repeated cell lookup | 0.4724 / 0.5146 | 0.4633 / 0.5054 |
| Sparse repeated cell lookup | 1.1908 / 1.2288 | 1.2275 / 1.2634 |

Row traversal improves about 30% dense and 42% sparse; complete process costs
improve about 25% and 39%. Individual lookup code is unchanged; its small
positive/negative sample differences do not establish a lookup optimization.
Kernel RSS remains about 32 MiB and there are no temporary files. The conservative
model ledger stays 256 bytes per cell plus the four-byte title (256,000,004 bytes
at this scale), which is not physical RSS.

Existing independent-map row/storage tests, workspace correctness, strict Clippy
and Rust 1.88 worksheet tests pass. This is a model-access result; parser and
Python conversion costs, and the calamine speed gap, remain separate work.
Raw evidence includes both sizes: [samples](results/alpha7-row-cursor.json).
