# M2 boolean checkpoint

Rust 1.88.0, openpyxl 3.1.5, release builds, native Linux wait4 RSS measurement. Raw results: [results/m2-boolean.json](results/m2-boolean.json). Three measured runs after one warm-up; before/after numeric runs alternate. This is a focused checkpoint measurement, not a replacement for the five-run M1 baseline.

| Workload (10 columns) | Median seconds | Median peak RSS MiB |
|---|---:|---:|
| Boolean, 10,000 rows | 0.0786 | 1.50 |
| Boolean, 100,000 rows | 0.7342 | 1.48 |
| Numeric, 100,000 rows, M1 | 0.6984 | 1.51 |
| Numeric, 100,000 rows, M2 | 0.7516 | 1.57 |
| Numeric, 1,000,000 rows, M1 | 6.9174 | 1.58 |
| Numeric, 1,000,000 rows, M2 | 7.4010 | 1.58 |

Boolean counts/types and true counts are asserted for every run. Numeric counts/checksums remain identical. The numeric hot path is approximately **7% slower** at one million rows; this is a measured regression to optimize, not a speed improvement. Compile-time numeric/boolean decoder specialization did not remove this cost. CellValue remains 16 bytes with no boolean heap payload; fixed-setting RSS is stable across the measured boolean sizes. No runtime temporary files are created.

Public openpyxl load_workbook probes, using generated workbooks with a replacement A1 value element, produced false for `0` and `-00`, true for `1`, `+002`, and a decimal literal larger than i64, None for an empty value, and ValueError for `true` and `1.0`. The Rust decoder preserves these outcomes through signed ASCII decimal validation and zero/nonzero detection without integer conversion. Permissive Python-specific syntax for malformed OOXML is not promised. Integration tests cover typed literals, XML entity decoding, empty values, malformed values with part/cell context, projection, materialization, Auto, and owned batches.

Reproduce by building the M1 sum executable at `74c082a58e8b` in a separate checkout and retaining its path, generating the existing numeric inputs as described in README.md, then running:

```sh
cc -O2 benchmarks/measure.c -o benchmarks/measure
cargo build --release --examples --locked
python benchmarks/boolean_checkpoint.py --numeric-before /path/to/m1-sum
```

The script generates boolean inputs through the public openpyxl write-only API. Library parsing uses the existing selected calamine port and the shared models; it does not call openpyxl or calamine at runtime. Integer precision, errors, strings/styles/dates/formulas, and disk-backed large-string support remain required before M2 is complete.
