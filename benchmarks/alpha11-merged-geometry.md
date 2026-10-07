# A11 development: sparse merged geometry

CPython 3.12.14, openpyxl 3.1.5 and a development CrabXL wheel using the local W12
core implementation based on `5b4d018717b8295e2ec8efef5a597c3942d6b22a`
with this checkpoint's block-bounded cleanup changes. The adapter is
`fbcc1734b03d9646e5f313b78816c8c1e279c425`. Package versions remain alpha.10 until consolidated release
acceptance. After one 8,000-region warmup per engine, three independent processes per
engine/size were measured with alternating engine order. The table reports
medians; these are not release-wide acceptance or general read/write speed claims.
No builds or tests overlapped the measurements. Both engines used their default
ordinary mode/compression settings.

The same script creates non-overlapping 2x2 merged regions three rows apart, with
one integer anchor per region. Each output's declarations, final anchor and finite
bounds are validated by reopening with openpyxl. Peak RSS is recorded before that
reference reopening. Preparation includes cell assignment and merging. Save time
includes ZIP creation; output size is measured separately. XML declaration count
is also checked. This is an in-memory ordinary model; temp-file peak is not measured.

| Engine | Merges | Prepare (s) | Save (s) | Peak RSS (KiB) | Output bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| CrabXL | 2,000 | 0.01270 | 0.00306 | 19,320 | 29,545 |
| openpyxl | 2,000 | 0.69966 | 0.03817 | 35,828 | 38,081 |
| CrabXL | 8,000 | 0.05543 | 0.01124 | 20,172 | 99,811 |
| openpyxl | 8,000 | 11.13531 | 0.14983 | 50,540 | 139,324 |

The shared interval index and default-style sparse output avoid repeated full
merge scans and covered-cell allocations. Physical cleanup now visits only
sparse blocks overlapping the changed rectangle, adjusts removed payload and
released capacities directly, and avoids a worksheet-wide recount. An earlier
single-sample implementation using full physical scans took 0.11587 s to prepare
8,000 regions; the new repeated median is about 0.05543 s. This limited comparison
is not a statistically controlled release speedup claim. The measurement does not
cover dense styled merged areas, loaded-source editing, calamine or rust_xlsxwriter comparisons.
Those retain their broader acceptance requirements.

Reproduce after building/installing the development wheel:

```sh
python benchmarks/merge_geometry.py crabxl 8000
python benchmarks/merge_geometry.py openpyxl 8000
```

[Raw samples](alpha11-merged-geometry.jsonl) retain measured values without rounding.
