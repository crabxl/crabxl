# A7 checkpoint: source-backed cell structure

This checkpoint supports canonical source-backed row/column insertion/deletion,
rectangle move/copy and translated normal-formula moves with preserving repeat
saves. It does not close M4, publish A7, or meet the calamine read-speed target.

Reproduce after building the release examples:

```sh
cargo build --release -p crabxl --example loaded_rows --example workbook_demo
python benchmarks/alpha6_loaded_bank.py --structure-only \
  --checkpoint 'A7 staged source-backed structural edits; not full M4 completion' \
  --output benchmarks/results/alpha7-loaded-structure.json
```

The generated input has two sheets, ten integer columns, and every physical
coordinate/count/checksum verified. The timed process opens and materializes both
models, inserts two leading rows and one leading column in the first sheet,
saves atomically, then streams every saved cell. It checks each shifted
coordinate and both complete checksums. Generation and compilation are excluded.
One warmup precedes three serial release runs. Rust 1.99, Linux two-CPU quota,
8-GiB cgroup limit; no compilation/tests/other benchmarks overlap measurements.
Raw runs, binary/input hashes and environment are in the JSON report.

| Rows per sheet | Total cells | Wall (s) | CPU (s) | Peak RSS (KiB) | Retained managed bytes | Output bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 10,000 | 200,000 | 0.4779 | 0.4777 | 9,600 | 51,214,059 | 607,689 |
| 100,000 | 2,000,000 | 4.9306 | 4.9300 | 66,940 | 512,014,059 | 5,895,313 |

These are supported-workflow measurements, not a before/after speedup or an
editable comparison with the read-only calamine range. Full models retain all
cells; their memory scales with cell count. Managed ledgers conservatively
charge 256 bytes per cell and additional structural working space, independently
of the packed storage's much smaller physical RSS. The example uses a 1-GiB
joint budget/materialization ceiling and 768-MiB per-sheet ceiling. The initial
large run hit its smaller existing per-sheet working cap; enlarging the explicit
allowance changes resource policy, not the allocation cost or archive limits.

No SST temp data is created. The only working file is the adjacent completed ZIP;
sampled peak file bytes equal the output sizes above and are a lower bound,
not complete filesystem I/O accounting. Output deletion and adjacent temp-file
cleanup are checked after every run. Repeated-save and output-failure guarantees
are covered separately by deterministic workflows.

Existing tests cover later edits/append, sparse shifts/moves/copies, normal formula
translation/cache invalidation, unchanged imported styles/opaque assets,
trailing empty rows, ISO and numeric dates, strict/prefixed namespaces,
relationship-resolved plain/rich SSTs, and explicit graph/bounds/work-budget
failures. Affected common/advanced graphs remain tracked M5/M6 dependencies;
loaded sheet creation/copy/removal and remaining M4 gates are unfinished.
