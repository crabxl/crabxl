# M4 existing-package sparse insertion

`upsert_value` inserts K1 and A1000001 into numeric ten-column files without allocating the intervening empty rows. Native wait4 measures one warmup and three runs; public openpyxl 3.1.5 verifies existing values, inserted values and dimensions outside timing.

| Original rows | Median wall time | Peak RSS | Charged patches | Output/temp ZIP |
|---|---:|---:|---:|---:|
| 10,000 | 0.704 s | 2.39 MiB | 792 B | 0.39 MiB |
| 100,000 | 6.627 s | 2.43 MiB | 792 B | 3.69 MiB |

No worksheet XML temporary files are used. The full adjacent output ZIP remains a disk cost; cleanup and checksums pass. New cells have default style. Existing-only set_value remains unchanged; upserts reject non-anchor merged cells and unsupported metadata at save. Source structural editing remains staged.

Reproduce with `cargo build --release -p openrsxl --examples --locked` then `python benchmarks/insertion_checkpoint.py`. [Raw runs](results/m4-insertion.json) include CPU time.

Namespace classification now uses one match instead of duplicated comparisons. [Alternating numeric runs](results/m4-namespace.json) compare the initial M4 binary with this isolated refactor before insertion changes: 100k medians 0.768/0.761 s; 1m 7.382/7.495 s. These show no consistent speed gain, so no optimization improvement is claimed; the initial M4 reader regression remains open.
