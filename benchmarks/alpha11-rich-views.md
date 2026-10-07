# A11 development: typed rich-value views

CPython 3.12.14, openpyxl 3.1.5, Rust 1.99 release build. The development wheel
is based on core `5b7e6deefed53d91544e3e33b509bdba0ab87bdb` and adapter
`fbcc1734b03d9646e5f313b78816c8c1e279c425` with this W13 checkpoint. Its SHA-256 is
`594b648a8dd57e8a17d4f4410d659186a1008786d41f6c4a1a8079bb13276490`.
Package numbers remain alpha.10; this is not release-wide acceptance.

Fixtures were generated in a separate process before measurement. A fresh
standard-library-only parent launched independent worker processes, keeping
fixture-generator memory out of inherited process high-water marks. One warmup
per engine/mode used 8,000 rows; three samples per engine/mode/size alternated
engine order. No builds or tests overlapped measurement. The table gives medians.
Peak RSS is sampled before importing/reopening the reference in CrabXL workers.

Every row has three rich runs, one bold ARGB font and one column. Create prepares
ordinary rich cells using the same public calls. Edit opens the same reference
fixture with rich preservation and scans every displayed value with a length
checksum. Both then mutate the last styled run, insert a leading row and save.
Reopening checks the final text, bold/color and row extent. Defaults are used;
peak temporary-file bytes are not measured. Scalar or plain read modes are not
comparable to this typed model workload.

| Mode | Engine | Rows | Prepare (s) | Edit (s) | Save (s) | RSS (KiB) | Bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| create | CrabXL | 2,000 | 0.03012 | 0.00033 | 0.00366 | 19,684 | 19,282 |
| create | openpyxl | 2,000 | 0.00671 | 0.00245 | 0.05366 | 32,952 | 22,843 |
| edit | CrabXL | 2,000 | 0.04972 | 0.00607 | 0.00589 | 19,984 | 20,406 |
| edit | openpyxl | 2,000 | 0.08531 | 0.00332 | 0.05960 | 35,140 | 22,851 |
| create | CrabXL | 8,000 | 0.11660 | 0.00076 | 0.01302 | 22,092 | 63,564 |
| create | openpyxl | 8,000 | 0.02253 | 0.01701 | 0.19832 | 37,756 | 75,909 |
| edit | CrabXL | 8,000 | 0.19350 | 0.02391 | 0.02135 | 22,564 | 64,720 |
| edit | openpyxl | 8,000 | 0.34751 | 0.00950 | 0.21711 | 44,788 | 75,917 |

CrabXL uses less RSS and lower complete measured operation time in these cases.
Typed load-plus-scan is faster here after sharing trusted font conversion. Rich
cell creation through Python remains slower than assigning reference objects,
and source structural edits retain validation cost. Cached immutable native run
fonts avoid repeated input dictionary conversion/validation. Native serialization
is faster in these cases. None of this establishes calamine/rust_xlsxwriter
superiority, general worksheet speed, complete rich baseline coverage or large
workload acceptance; the remaining costs stay in A11/A16 profiling.

Reproduce after installing the checkpoint wheel; generate fixtures separately:

```sh
python benchmarks/rich_views.py openpyxl fixture 2000
python benchmarks/rich_views.py openpyxl fixture 8000
python benchmarks/rich_views.py crabxl edit 8000
python benchmarks/rich_views.py openpyxl edit 8000
```

[Raw samples](alpha11-rich-views.jsonl) retain unrounded values.
