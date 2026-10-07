# A11 development: sparse merged geometry

CPython 3.12.14, openpyxl 3.1.5 and a development CrabXL wheel using the local W12
core implementation. Package versions remain alpha.10 until consolidated release
acceptance. These are single samples per engine/size, not release-wide performance
acceptance or general read/write speed claims. No builds or tests overlapped the
measurements. Both engines used their default ordinary mode/compression settings.

The same script creates non-overlapping 2x2 merged regions three rows apart, with
one integer anchor per region. Each output's declarations, final anchor and finite
bounds are validated by reopening with openpyxl. Peak RSS is recorded before that
reference reopening. Preparation includes cell assignment and merging. Save time
includes ZIP creation; output size is measured separately. XML declaration count
is also checked. This is an in-memory ordinary model; temp-file peak is not measured.

| Engine | Merges | Prepare (s) | Save (s) | Peak RSS (KiB) | Output bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| CrabXL | 2,000 | 0.01698 | 0.00298 | 19,200 | 29,545 |
| openpyxl | 2,000 | 0.68731 | 0.03926 | 35,916 | 38,078 |
| CrabXL | 8,000 | 0.11587 | 0.01019 | 20,072 | 99,811 |
| openpyxl | 8,000 | 10.85803 | 0.16022 | 50,456 | 139,324 |

The shared interval index and default-style sparse output avoid repeated full
merge scans and covered-cell allocations. The measurement does not cover dense
styled merged areas, loaded-source editing, calamine or rust_xlsxwriter comparisons.
Those retain their broader acceptance requirements.

Reproduce after building/installing the development wheel:

```sh
python benchmarks/merge_geometry.py crabxl 8000
python benchmarks/merge_geometry.py openpyxl 8000
```

[Raw samples](alpha11-merged-geometry.jsonl) retain measured values without rounding.
