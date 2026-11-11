# Roadmap

CrabXL has the capabilities of openpyxl 3.1.5. Each release and its changes
are in the [changelog](../CHANGELOG.md). The Python package has its own
[roadmap](https://github.com/crabxl/crabxl-python/blob/main/internal/roadmap.md).

## Next

1. Open written and edited files in Excel for macOS. Windows Excel already opens
   written and edited 1,000,000-row files with the right rows and date formats.

## Not provided

- Formula calculation. CrabXL keeps the cached results that Excel saved.
- Legacy `.xls` and binary `.xlsb` files.

## Performance measurements

Measured before 0.1.0 on one Windows computer, 1,000,000 rows x 10 columns.
These are development measurements, not published benchmarks.

| Scenario | CrabXL | Reference |
|---|---|---|
| Read (streaming) | 2.6 s, 56 MiB | calamine 3.8 s, 750 MiB |
| Write | 5.5 s, 11 MiB | rust_xlsxwriter 9.1 s, 1,354 MiB; constant-memory mode 8.1 s, 9 MiB |
| Edit (load, change, save) | 10.0 s, 75 MiB | umya-spreadsheet 49.8 s, 3,859 MiB |

rust_xlsxwriter's constant-memory mode uses about 2 MiB less peak memory than
CrabXL's write, because CrabXL's compression threads each keep a deflate state.
