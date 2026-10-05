# A6 lazy loaded bank ownership checkpoint

This checkpoint materializes original worksheets lazily into the canonical bank,
transfers source style payloads, and coordinates models with source catalogs and
SST/cache storage. It does not yet implement original-package model mutation or
complete A6/M4. See [ADR 0056](../docs/decisions/0056-lazy-source-backed-workbook-bank.md).

## Numeric ownership-path comparison

Both paths use the same current parser and sparse `Worksheet` cell type. The
standalone path reproduces the former native materialization algorithm with
independent per-sheet limits. The bank path additionally performs joint source/
model accounting, stable-ID commits, and source-catalog reference checks.
This is a native full-model comparison, not streaming or Python API evidence.

Linux release binaries, Rust 1.99; one warmup and three rotating serial measured
runs. Generation/build are excluded. The native wait4 launcher reports total
process wall/CPU time and kernel peak RSS, including startup and destruction.
Every run checks complete integer cell counts and checksums. Two sheets each
contain consecutive integer data in ten columns.

| Rows per sheet | Cells total | Path | Median wall | Peak RSS | Managed retained bytes |
| --- | --- | --- | --- | --- | --- |
| 10,000 | 200,000 | Canonical lazy bank | 0.186321 s | 17,600 KiB | 51,210,508 |
| 10,000 | 200,000 | Standalone models | 0.176769 s | 17,564 KiB | 51,205,605 |
| 100,000 | 2,000,000 | Canonical lazy bank | 1.895886 s | 157,644 KiB | 512,010,508 |
| 100,000 | 2,000,000 | Standalone models | 1.798336 s | 157,596 KiB | 512,005,605 |

The added checks cost approximately 5.4% wall time on these cases; this is an
explicit A7 profiling item, not an optimization claim. Managed accounting adds
4,903 bytes at both sizes. The conservative node allowance is deliberately larger
than actual measured RSS. Neither case creates an SST or temporary source store;
measured SST temporary bytes are zero. This does not establish zero disk use for
text fixtures or preservation/editor output.

Raw samples, file/binary SHA-256 values, toolchain/platform and cgroup constraints
are in [alpha6-loaded-bank.json](results/alpha6-loaded-bank.json).

## Low-memory strings and failure acceptance

The deterministic integration fixture contains 3,000 unique 128-byte SST values
used by two sheets under one 4 MiB operation allowance. Forced RAM rejects the
second model and retains the first; Auto spills the existing RAM table to owned
disk storage, and explicit Disk loads both. Actual values, disk lookups and
nonzero temporary bytes are asserted. Linux checks open temporary handles,
including anonymous files that do not appear in directory listings; destruction
cleans up owned resources. These are resource/correctness cases, not timed
benchmark results or whole-process RSS guarantees.

## Reproduction

```sh
cargo build --release --locked -p crabxl --example loaded_rows --example workbook_demo
cc -O2 -Wall -Wextra -Werror benchmarks/measure.c -o benchmarks/measure
python benchmarks/alpha6_loaded_bank.py
cargo test -p crabxl-xlsx --test loaded --locked
```
