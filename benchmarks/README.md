# Benchmarks

One suite compares CrabXL with the main Rust XLSX libraries on three tasks.
The latest results are in [results/results.md](results/results.md).

| Scenario | Task | Libraries |
|---|---|---|
| Read | Open the file and visit every cell value. | Streaming: crabxl (`WorkbookReader`), calamine (`worksheet_cells_reader`), oxlsx (`open_readonly`). Whole sheet: crabxl (`LoadedWorkbook`), calamine (`worksheet_range`), umya-spreadsheet. |
| Write | Create the sheet and save it. | crabxl (streaming writer), rust_xlsxwriter (normal and constant-memory mode), oxlsx, umya-spreadsheet |
| Edit | Load the file, change one cell in every 100 rows, append 100 rows and save. | crabxl (`LoadedWorkbook`), oxlsx, umya-spreadsheet |

calamine only reads and rust_xlsxwriter only writes, so they are not in the
other scenarios.

## Data

The driver generates the input with rust_xlsxwriter, so no library reads its
own output. Each row has 10 cells: an integer, a decimal number, a repeated
text (1,000 different values), a unique text, a date, a boolean and four
decimal numbers. Three sizes:

| Size | Rows | Cells |
|---|---:|---:|
| small | 10,000 | 100,000 |
| medium | 100,000 | 1,000,000 |
| large | 1,000,000 | 10,000,000 |

The crabxl-python repository uses the same data shape for the Python
libraries.

## Method

- Each case runs in a new process. Time is the wall time of the whole process,
  including start-up. Memory is the peak resident memory of the process
  (`PeakWorkingSetSize` on Windows, `ru_maxrss` on Linux and macOS).
- Each case runs one time to warm the file cache, then three times. The
  tables show the median of the three.
- Run the suite on an idle computer that is connected to power. Do not run
  builds or tests at the same time.
- Every case prints the number of cells it handled. The suite records it so
  that you can check that all libraries did the same work.
- A case that takes more than 15 minutes is shown as a timeout. If a case takes
  more than 90 seconds, it is not run at the next size (10 times more rows) and
  is shown as an estimated timeout.
- All libraries are built in release mode with the same profile (`lto =
  "thin"`, one codegen unit).

## Run

Requirements: Rust 1.88 or later and Python 3.11 or later. The suite builds
the driver in `driver/`, which has its own Cargo workspace.

```sh
python benchmarks/run.py
python benchmarks/run.py --sizes small medium --runs 1
```

Generated input files are kept in `benchmarks/data/` (ignored by Git).

## Files

| File | Use |
|---|---|
| `run.py` | Runs the cases and writes `results/`. |
| `driver/src/main.rs` | One case per process for each library, and the input generator. |
| `results/results.json` | Raw results. |
| `results/results.md` | Tables and charts. |
