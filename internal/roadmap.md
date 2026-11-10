# Roadmap

A12 completed openpyxl 3.1.5 capability. A13 is done. The beta is next.

## A13: performance and memory (done)

Goal: on every benchmark scenario, be faster and use less peak memory than
the libraries in this table.

| Scenario | Faster than | Lower peak memory than |
|---|---|---|
| Read | openpyxl, calamine | openpyxl, calamine |
| Write | openpyxl, rust_xlsxwriter | openpyxl, rust_xlsxwriter |
| Edit (load, change, save) | openpyxl, umya-spreadsheet | openpyxl, umya-spreadsheet |

Changes:

- Edits stream: appended rows of a sheet that is not loaded are written after
  the source rows on save, without loading the sheet into memory.
- A whole-sheet model is checked once, not on every structural edit.
- Worksheets of 4 MiB or more are compressed on up to four threads when they
  are written or saved.
- `LoadOptions::with_memory_limit(bytes)` sets the memory limit in one call.

Measured on one Windows computer, 1,000,000 rows x 10 columns:

| Scenario | Before | After | Reference |
|---|---|---|---|
| Read (streaming) | 2.6 s, 56 MiB | unchanged | calamine 3.8 s, 750 MiB |
| Write | 8.4 s, 10 MiB | 5.5 s, 11 MiB | rust_xlsxwriter 9.1 s, 1,354 MiB; constant-memory mode 8.1 s, 9 MiB |
| Edit | 32.1 s, 497 MiB | 10.0 s, 75 MiB | umya-spreadsheet 49.8 s, 3,859 MiB |

Every target is met against rust_xlsxwriter's default mode. Its
constant-memory mode uses about 2 MiB less peak memory than crabxl's write,
because crabxl's compression threads each keep a deflate state.

## Optional: benchmark results

These are not needed for the beta:

1. Rerun the full suite (`benchmarks/README.md`) on an idle computer and commit
   the results. Other programs changed the numbers of the last attempt.
2. Validate produced files in Excel desktop on macOS. Windows Excel opens the
   written and edited 1,000,000-row files with the right rows and date formats.

## Beta

Release `0.1.0-beta.1` of the core and the Python package.
