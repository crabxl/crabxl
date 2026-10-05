# Source-backed lazy title changes

The original-source title overlay changes the display name and active selection
without decoding worksheet cells. The release probe then saves and streams all
values from the saved workbook, verifying special-character/Unicode name,
stable active identity, complete cell count and exact checksum. The headline
includes source opening, metadata changes, ZIP output and full value readback;
it is not the latency of the title setter or a parser-only comparison.

Rust 1.99, release defaults including zlib-rs, Linux x86-64, two-CPU quota.
One warmup and three serial runs per size; generation, builds and tests excluded.
The source has two sheets with ten numeric columns. No retained worksheet models
or SST temporary files are created; the original ZIP source remains borrowed by
the coordinator until it is dropped. Output is a temporary adjacent ZIP and
the completed target is deleted and cleanup checked after each measurement.

| Rows per sheet | Verified cells | Median wall (s) | Peak RSS (KiB) | Managed retained bytes | Sampled working file bytes |
|---|---:|---:|---:|---:|---:|
| 10,000 | 200,000 | 0.1034 | 2,848 | 14,272 | 606,838 |
| 100,000 | 2,000,000 | 1.0387 | 2,976 | 14,272 | 5,882,422 |

Working-file sampling is a lower bound on transient peak storage. Managed
retained accounting is not whole-process RSS. These measurements establish a
bounded usable title/save/readback workflow, not a before/after optimization or
calamine superiority. Full-model/Python speed targets and remaining M4 mutations
remain unresolved. Workspace tests, strict Clippy and Rust 1.88 loaded tests pass.
Raw samples, binary and input hashes: [report](results/alpha7-lazy-renaming.json).
