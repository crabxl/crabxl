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
decimal numbers. The input uses a shared string table, as in files that Excel
saves. Six sizes:

| Size | Rows × columns | Cells | File | Scenarios |
|---|---:|---:|---:|---|
| small | 10,000 × 10 | 100,000 | 0.6 MB | all |
| medium | 100,000 × 10 | 1,000,000 | 6 MB | all |
| large | 1,000,000 × 10 | 10,000,000 | 60 MB | all |
| xl | 1,000,000 × 100 | 100,000,000 | about 370 MB | read |
| xxl | 1,000,000 × 280 | 280,000,000 | about 1 GB | read |
| nyc311 | 1,000,001 × 41 | 28,056,975 non-empty | 181 MB | read |

Sheets wider than 10 columns repeat the same 10-column pattern. The xl, xxl
and nyc311 files are read only, by the readers that can hold them in memory.

## NYC 311 file

The nyc311 size is the real data set that the
[calamine README](https://github.com/tafia/calamine#performance) uses: one
million New York City 311 service requests with 41 columns, mostly text, with
empty fields. To build it:

1. Download
   [NYC_311_SR_2010-2020-sample-1M.7z](https://raw.githubusercontent.com/wiki/jqnatividad/qsv/files/NYC_311_SR_2010-2020-sample-1M.7z)
   (48 MB) and extract the CSV into `benchmarks/data/nyc311/`.
2. Open the CSV in Excel and save it as `benchmarks/data/nyc311.xlsx`. Without
   Excel, run `driver csv-fixture <csv> benchmarks/data/nyc311.xlsx`.
3. Write the number of non-empty CSV fields (28056975) to
   `benchmarks/data/nyc311.cells`.

Readers count non-empty cells only, because libraries return empty cells
differently. All readers read the first worksheet.


The crabxl-python repository uses the same data shape for the Python
libraries.

## Method

- Each case runs in a new process. Time is the wall time of the whole process,
  including start-up. Memory is the peak resident memory of the process
  (`PeakWorkingSetSize` on Windows, `ru_maxrss` on Linux and macOS).
- Each case runs two ways. **All CPUs**: the library can use every logical CPU,
  as in normal use. **One CPU**: the process starts suspended, is pinned to the
  same single logical CPU for every library, and only then runs. This shows
  how fast a library is without extra threads. The tables also show the CPU
  time (user plus kernel) of the process.
- Each case runs one time to warm the file cache, then three times (one time
  if the warm-up takes more than 10 seconds). The
  tables show the median of the three.
- Safety limits that would reject the large files are raised for every library
  that has them (crabxl memory limit, wolfxl size limits).
- Run the suite on an idle computer that is connected to power. Do not run
  builds or tests at the same time.
- Every case prints the number of cells it handled. The suite records it so
  that you can check that all libraries did the same work.
- A case that takes more than 5 minutes is shown as a timeout. If a case takes
  more than 30 seconds, it is not run at the next size (10 times more rows) and
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
