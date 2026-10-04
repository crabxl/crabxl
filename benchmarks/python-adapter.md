# Direct Python adapter comparison

Both engines execute the same supported public calls. Creation uses Workbook/active/append/save; editing uses load_workbook, Sheet/A1.value and save/close. The Rust adapter preserves loaded parts lazily; openpyxl general mode materializes them. This difference is included in measured costs and is not equivalent full feature coverage.

CPython 3.12, openpyxl 3.1.5, PyO3 0.26, Rust 1.88 release with thin LTO. Native wait4 records wall/CPU/peak RSS. One warmup and three alternating runs per engine. Checksums and cleanup assertions are outside timing; RSS includes interpreter/module/conversion costs.

| Rows | Operation | Adapter median | openpyxl median | Adapter peak RSS | openpyxl peak RSS |
|---|---|---:|---:|---:|---:|
| 10,000 | create | 0.148 s | 0.666 s | 18.07 MiB | 66.57 MiB |
| 10,000 | edit | 0.718 s | 1.137 s | 10.51 MiB | 82.55 MiB |
| 100,000 | create | 1.383 s | 6.378 s | 86.31 MiB | 371.96 MiB |
| 100,000 | edit | 6.590 s | 12.924 s | 10.46 MiB | 529.87 MiB |

At 100k rows creation is about 4.6x faster and lazy edit/save about 2.0x faster on this workload. Creation retains a sparse Rust model (86.31 MiB RSS), unlike Rust sequential-only creation. Lazy single-cell editing uses 10.46 MiB including Python and does not materialize a worksheet. These results do not cover binding read iteration, styled model loading or million-row Python workloads.

Temporary storage is polled every 10 ms and may miss brief peaks. At 100k creation, observed native/Python temp peaks are 34.15/37.07 MiB; native output is 2.81 MiB versus 3.00 MiB. Native creation includes worksheet XML spools plus an adjacent output ZIP. At 100k edit, observed native/Python temp peaks are 3.67/37.07 MiB; native output is 3.69 MiB versus 3.00 MiB. Native edited output needs its full resulting ZIP temporarily, with an old target retained until replace. Dependency compression and preservation strategies produce different file sizes. OS file cache/tmpfs costs are not included in process RSS.

[Raw runs](results/python-adapter.json) include CPU, output bytes, sampled temp bytes, checksums and cleanup. Reproduce after installing the release adapter wheel with `python benchmarks/python_adapter_checkpoint.py`; numeric inputs are the existing benchmarks/data/numbers-* fixtures. `python_adapter_run.py` imports only the selected engine. The adapter remains partially compatible and the full openpyxl test suite is not an acceptance claim.
