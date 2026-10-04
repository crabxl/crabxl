# Owned workbook checkpoint

This is explicit sparse scalar/formula model creation, an independent whole-sheet copy, reorder/rename/active selection and borrowed export. It does not establish loaded feature-graph structural editing or complete M4. Native Rust versus Python is a separate measurement from the same-call Python adapter regression.

Reproduce after release example and adapter builds:

```sh
python benchmarks/workbook_checkpoint.py --before-python-path /path/to/c50eca7/package-snapshot
```

CPython 3.12, openpyxl 3.1.5, Rust 1.88 release. One warmup and three alternating runs; kernel wait4 measures elapsed/CPU/peak RSS. Temporary worksheet spools are polled at 10 ms; final output ZIP is excluded. Public openpyxl read-only output checks every numeric cell in both copies, sheet order and active selection. Cleanup passes after each run. Output/input semantics are equivalent for this numeric workload; serializers need not emit identical ZIP bytes.

| Rows per sheet | Engine | Wall seconds | CPU seconds | RSS MiB | Observed temp MiB | Output MiB |
|---|---|---:|---:|---:|---:|---:|
| 10,000 | openrsxl-native | 0.199 | 0.198 | 17.25 | 5.87 | 0.58 |
| 10,000 | openpyxl | 1.325 | 1.488 | 91.97 | 3.51 | 0.62 |
| 100,000 | openrsxl-native | 2.264 | 2.212 | 153.96 | 62.69 | 5.61 |
| 100,000 | openpyxl | 12.780 | 12.962 | 603.50 | 37.07 | 5.99 |

Both copied sheets remain resident: this is not constant-memory model loading. At 100,000 rows per sheet, conservative managed accounting is 512,000,523 bytes for two million physical cells; measured RSS is lower because the allowance deliberately estimates BTree nodes conservatively. The explicit copy duplicates payloads, as required for independent editable models. This numeric sample meets the faster-than-openpyxl target and desired lower RSS; no calamine comparison applies to creation/copy and no rust_xlsxwriter comparison is claimed here.

| Python create rows | Before c50eca7 wall seconds | Current wall seconds | Change |
|---|---:|---:|---:|
| 10,000 | 0.1484 | 0.1458 | -1.8% |
| 100,000 | 1.4171 | 1.3642 | -3.7% |

The Python default-active create regression does not exercise the new aggregate bank; it checks that shared writer/metadata changes preserve existing default behavior. These small timing differences do not establish a general optimization. Checksums and cleanup pass. Existing reader slowdown from earlier checkpoints remains unresolved; metadata parsing does not alter the numeric row decoder. The Python bank migration and other workloads require further measurements.

[Raw runs](results/m4-workbook.json) include per-stage build/copy/write times and all metrics.
