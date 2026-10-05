# A6 loaded source coordinator checkpoint

The lazy canonical bank now owns one preserving source editor. Pending value
overlays update cached models or apply during later materialization; catalogs,
inventory, models and overlays share a managed allowance. This is the scalar
integration checkpoint in [ADR 0057](../docs/decisions/0057-preserving-loaded-bank-overlays.md),
not full A6/M4 acceptance or Python binding evidence.

## Equivalent model loading and complete edited-output verification

Rust 1.99 release binaries run serially on Linux, with no simultaneous builds or
tests. Inputs contain two identical integer worksheets with ten columns. Each
mode warms up once, then runs three times in rotating order. The native wait4
launcher records process wall/CPU time and kernel peak RSS, including teardown.
Input generation and builds are excluded. Every run checks all cells and the
integer checksum; numeric inputs do not need an SST temporary store.

| Rows per sheet | Mode | Median wall | Peak RSS | Managed retained bytes | Completed output ZIP bytes |
| --- | --- | --- | --- | --- | --- |
| 10,000 | Canonical bank load | 0.173665 s | 17,932 KiB | 51,213,515 | 0 |
| 10,000 | Standalone load | 0.168522 s | 17,996 KiB | 51,205,605 | 0 |
| 10,000 | Bank load/edit/save/reload | 0.583599 s | 18,500 KiB | 51,214,051 | 606,976 |
| 100,000 | Canonical bank load | 1.774224 s | 158,016 KiB | 512,013,515 | 0 |
| 100,000 | Standalone load | 1.767422 s | 157,964 KiB | 512,005,605 | 0 |
| 100,000 | Bank load/edit/save/reload | 6.293689 s | 158,532 KiB | 512,014,051 | 5,887,308 |

Standalone and bank loading use the same current parser and sparse model. Bank
loading adds source editor ownership, joint accounting and stable model commit.
Its measured wall overhead is about 3.1% and 0.4% at the two scales, respectively;
sample variation prevents inferring a reliable improvement over the earlier
ownership checkpoint. The managed increment is 7,910 bytes at either scale.
Conservative node charges are larger than actual RSS and do not cap whole-process
RSS. Remaining throughput/accounting costs stay in the A7 profiling backlog.

The edit mode changes the first sheet's A1 to 42, saves through an adjacent
guarded ZIP, then streams the entire saved workbook to verify the updated checksum
and unchanged count. Its timing includes this verification and is therefore a
different workload from read-only model loading. The completed ZIP size is actual
disk output, not a sampled peak temporary-storage measurement. The harness checks
adjacent temporary cleanup and deletes each completed benchmark output afterward.
SST temporary bytes are zero for these numeric cases. Low-memory text storage and
failure cleanup remain verified by the loaded integration tests, not this timing
fixture.

Raw repeated samples, CPU time, binary/input hashes, toolchain and cgroup limits
are in [alpha6-loaded-coordinator.json](results/alpha6-loaded-coordinator.json).

## Reproduction

```sh
cargo build --release --locked -p crabxl --example loaded_rows --example workbook_demo
cc -O2 -Wall -Wextra -Werror benchmarks/measure.c -o benchmarks/measure
python benchmarks/alpha6_loaded_bank.py --edit --output benchmarks/results/alpha6-loaded-coordinator.local.json
cargo test -p crabxl-xlsx --test loaded --locked
```
