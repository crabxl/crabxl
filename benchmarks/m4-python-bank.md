# Python aggregate workbook checkpoint

This workload uses identical public Python calls for Workbook, append, copy_worksheet, move_sheet, title, active and save. The two independent numeric sheets have identical values, order and active selection on readback. This measures the binding and interpreter costs, not just native Rust. It does not establish style/drawing/loaded feature-copy compatibility or full M4 acceptance.

CPython 3.12, openpyxl 3.1.5 and Rust 1.88 release. One warmup and three alternating runs. wait4 records wall/CPU/peak RSS; worksheet temporary storage is sampled every 10 ms, excluding the final output ZIP. Every output cell is verified outside timing through openpyxl read-only iteration, plus sheet names, active selection and cleanup.

| Rows per sheet | Python engine | Wall seconds | CPU seconds | RSS MiB | Observed temp MiB |
|---|---|---:|---:|---:|---:|
| 10,000 | crabxl | 0.290 | 0.247 | 25.86 | 6.43 |
| 10,000 | openpyxl | 2.057 | 1.424 | 92.10 | 3.51 |
| 100,000 | crabxl | 2.459 | 2.458 | 162.38 | 68.28 |
| 100,000 | openpyxl | 12.923 | 12.998 | 603.48 | 37.07 |

The 100k sample meets the required faster-than-openpyxl target and desired lower RSS. It uses more worksheet temporary space: both Rust sheet spools remain until ZIP packaging, while the Python reference serializes each materialized sheet separately. Full independent copies necessarily retain two models. No calamine or rust_xlsxwriter comparison is implied for this Python copy/edit workload.

| One-sheet create rows | Before 1f7e870 wall seconds | Current wall seconds | Change |
|---|---:|---:|---:|
| 10,000 | 0.1478 | 0.1475 | -0.2% |
| 100,000 | 1.3563 | 1.4253 | +5.1% |

At 100k the new bank costs about 5.1% elapsed and 4.4% CPU relative to earlier per-model handles. Aggregate checks and safe bank locking add work; this regression remains an optimization item rather than an improvement claim. It still comfortably exceeds the recorded equivalent openpyxl create workload. Additional sheets are scanned when acquiring a mutation facade; native bulk fill can retain one facade, whereas Python per-call edits currently reacquire it. Large sheet-count workloads remain unmeasured.

New registered Python sheets share the managed bank allowance. Removed caller-retained and directly constructed standalone sheets leave that scope. Loaded workbooks retain separate overlay/per-model allowances; Python object heap, allocator overhead and dependency allocations are not a hard aggregate RSS cap.

```sh
python benchmarks/workbook_checkpoint.py --python-adapter --before-python-path /path/to/1f7e870/package-snapshot
```

[Raw evidence](results/m4-python-bank.json) includes all runs, per-stage timing, checksums, output sizes and cleanup.
