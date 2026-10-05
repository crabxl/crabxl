# Source-backed lazy ordering

The release workflow opens a two-sheet numeric source, changes its second title,
moves that stable identity to the first display position, selects it explicitly,
saves and streams every value from the saved package. Count, exact checksum,
title and active index are verified. No worksheet models or SST temporary files
are created; original parts keep their identities and compressed payloads.

Rust 1.99 release defaults, zlib-rs, Linux x86-64 and two-CPU quota. One warmup
and three serial samples per size; generation, builds and tests are excluded.
Headline wall time includes save and full streaming readback, not only the move
method. Working files are sampled; completed output and cleanup are checked.

| Rows per sheet | Verified cells | Median wall (s) | Peak RSS (KiB) | Managed retained bytes | Sampled working file bytes |
|---|---:|---:|---:|---:|---:|
| 10,000 | 200,000 | 0.1005 | 2,992 | 14,709 | 606,837 |
| 100,000 | 2,000,000 | 1.0017 | 2,864 | 14,709 | 5,882,421 |

The managed ledger remains constant as cell count grows because only source
metadata/declarations are retained. It is not a process RSS cap. Sampled working
storage is a lower bound; output is an adjacent temporary ZIP followed by the
completed target. This is a new operation measurement, not a before/after speed
claim or calamine comparison. Workspace tests, strict Clippy and Rust 1.88 loaded
tests pass. Local defined-name owner remapping remains explicitly unsupported
until M5; remaining M4 sheet and structural operations are open.

All samples and binary/input hashes: [report](results/alpha7-lazy-reordering.json).
