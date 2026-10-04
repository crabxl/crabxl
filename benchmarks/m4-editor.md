# M4 sparse/editor release checkpoint

This measures a first M4 checkpoint, not full existing-file structural editing. Rust 1.88.0, openpyxl 3.1.5, Linux and AMD EPYC 9V74; native wait4 records CPU/wall and peak RSS. Rust uses one warmup plus three runs; Python general-mode load/edit/save uses one run without warmup. Checksums/public readback are outside timing. Inputs contain ten numeric columns.

| Rows | Rust existing-cell edit/save median | Rust peak RSS | Python load/edit/save | Python peak RSS |
|---|---:|---:|---:|---:|
| 10,000 | 0.669 s | 2.34 MiB | 1.153 s | 82.52 MiB |
| 100,000 | 6.694 s | 2.34 MiB | 12.631 s | 529.82 MiB |
| 1,000,000 | 63.888 s | 2.37 MiB | Not measured | Not measured |

The Rust editor queues one existing A1 value and rewrites worksheets/workbook for cache invalidation. Python materializes the general workbook before saving; these are different strategies with the same measured numeric edit outcome, not equivalent full feature coverage. Python million-row editing was not measured. Unchanged Rust passthrough medians are 0.0014/0.0033/0.0173 seconds at 10k/100k/1m rows: this copies compressed entries and does not parse/CRC-validate all payloads.

Reader regression at 100,000 rows: M3 0.699 s, M4 0.743 s (6.4% slower). Namespace context adds work; reducing this cost remains an optimization item.

Reader regression at 1,000,000 rows: M3 7.160 s, M4 7.606 s (6.2% slower). Namespace context adds work; reducing this cost remains an optimization item.

Sparse build plus row insert/delete takes 0.031 s / 8.57 MiB at 100k cells and 0.372 s / 76.95 MiB at one million cells. This explicit owning model scales with data; its conservative charged budget differs from RSS. Lazy single-cell overlays charge 536 bytes on these files, with catalog/XML working memory additional.

Edited output grows to about 0.39/3.69/36.64 MiB at 10k/100k/1m rows (exact byte sizes in raw results). Atomic path output needs the full resulting ZIP temporarily, with the old target retained until replace. No worksheet XML temporary file is used. Native temp polling every 25 ms can miss short-lived files; logical output-temp bytes are reported exactly. Python worksheet XML temp costs and output bytes are recorded separately. Cleanup checks pass.

Public generated fixtures verify styles, dates, comments, hyperlinks, merges, dimensions, validation, images, core properties, unknown part bytes, unchanged relationship identities, absent old formula cache and recalculation flags. Repeated edits/saves preserve assets without rereading them into an image cache. Macro/template payload fixtures are covered by Rust tests. No Excel GUI or full schema validation is claimed.

Raw runs, CPU, RSS, temp/output sizes and assertions: [results/m4-editor.json](results/m4-editor.json).

```sh
cargo build --release -p crabxl --examples --locked
# Preserve the previous M3 sum binary before rebuilding to reproduce regression comparisons.
python benchmarks/editor_checkpoint.py --before-reader /path/to/m3-sum
```
