# A6 lazy active-selection measurements

The source coordinator can select the second worksheet without materializing
either sheet, then save only changed workbook metadata. This is the active-view
checkpoint in [ADR 0058](../docs/decisions/0058-lazy-active-workbook-selection.md),
not full workbook structural acceptance.

## Active-only edit, save and complete readback

Rust 1.99 Linux release binaries; no builds/tests overlap measurements. Each
size warms up once, then runs three times in rotating order with bank-loading,
standalone-loading and model-edit comparison modes. Generation/builds are
excluded. Native wait4 measures process wall/CPU and kernel peak RSS including
startup and teardown. Inputs have two identical integer sheets, ten columns.

The active mode verifies the saved active index and streams all saved cells to
check the original complete count/checksum. Its total time therefore includes
full output readback, though the retained loaded bank stays at zero physical
cells before and after saving. This workload differs from full-model loading or
cell mutation; timings do not establish an interchangeable performance ratio.

| Rows per sheet | Verified cells | Median wall | Median CPU | Peak RSS | Retained source/bank allowance | Completed adjacent ZIP bytes |
| --- | --- | --- | --- | --- | --- | --- |
| 10,000 | 200,000 | 0.142726 s | 0.142567 s | 2,864 KiB | 13,803 bytes | 606,772 |
| 100,000 | 2,000,000 | 1.343381 s | 1.342876 s | 2,864 KiB | 13,803 bytes | 5,882,356 |

Retained managed bytes and measured peak RSS stay flat at these two numeric
scales. This does not cap process RSS or prove every metadata/asset workload has
the same footprint. Original payloads remain seekable/raw-copied; saving uses an
adjacent guarded ZIP. The completed ZIP size is actual disk output, not a sampled
peak temporary-space measurement. The harness verifies adjacent temporary
cleanup, then deletes completed outputs. Numeric cases have zero SST temporary
bytes; string and graph resources retain their separate acceptance tests.

The same raw report contains all comparison-mode samples; bank load still pays
joint checks and has variable overhead relative to standalone loading. No general
read optimization is claimed. The A7 native/Python throughput backlog stays open.

Raw hashes, platform/cgroup constraints, settings and repeated samples are in
[alpha6-active-selection.json](results/alpha6-active-selection.json).

## Reproduction

```sh
cargo build --release --locked -p crabxl --example loaded_rows --example workbook_demo
cc -O2 -Wall -Wextra -Werror benchmarks/measure.c -o benchmarks/measure
python benchmarks/alpha6_loaded_bank.py --edit --active --output benchmarks/results/alpha6-active-selection.local.json
cargo test -p crabxl-xlsx --test loaded --test editor --locked
```
