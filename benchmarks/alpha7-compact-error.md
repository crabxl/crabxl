# Compact failure context and mixed parser measurements

Private error context moves behind one owned box; categories, source chains,
part/cell getters and Display/Debug diagnostics remain unchanged. A separate
Rust 1.99 `size_of` probe on x86_64 confirms Error shrinks from 80 to 8 bytes
and `Result<CellValue>` from 80 to 16 bytes. Successful parsing allocates no
error; actual failures allocate an additional context box.

## Paired release method

Prior core `5add005` and the candidate use identical native worker/probe source,
lockfile and dependency features. One warmup and three rotating serial samples,
with every value/coordinate and applicable style verified. Calamine retains all
returned ranges; CrabXL retains editable models/source catalogs. Build, tests,
generation and separate CPU sampling do not overlap timings. Raw reports record
hashes, wall/CPU, kernel RSS, sampled working files and cleanup. The numeric
report records the current two-CPU quota; single-worker timings stay serial.

## Numeric and retained text results

Median wall seconds / RSS KiB at two million numeric cells or one million text
cells, respectively:

| Workload | Prior CrabXL | Compact error | Calamine |
| --- | --- | --- | --- |
| Numeric row stream | 1.0979 / 5,184 | 1.1588 / 4,996 | 0.4250 / 4,548 |
| Retain both numeric models/ranges | 1.3296 / 69,184 | 1.3164 / 68,804 | 0.5117 / 106,004 |
| Repeated text, retained RAM SST | 0.7972 / 68,544 | 0.8158 / 68,548 | 0.4408 / 160,784 |
| Unique text, retained RAM SST | 1.5342 / 224,832 | 1.4243 / 224,452 | 0.7592 / 309,168 |
| Repeated text, retained disk SST | 0.8118 / 68,800 | 0.8180 / 68,740 | Different storage policy |
| Unique text, retained disk SST | 2.1131 / 209,344 | 2.0356 / 209,156 | Different storage policy |
| Mixed styled retained model | 0.7809 / 68,544 | 0.7719 / 68,228 | Different typed/date contract |

Results are mixed: unique RAM text improves about 7%, but numeric streaming is
about 6% slower in these samples and repeated RAM text about 2% slower.
RSS changes are small; no broad RAM reduction or universal speed gain is claimed.
All native calamine speed targets remain unmet. The change primarily reduces
successful Result representation size while retaining complete failure context.

## Row streams and temporary files

At one million cells, prior/candidate median seconds: repeated RAM 0.7062/0.7066,
repeated disk 0.6980/0.7278, unique RAM 1.3490/1.3426, unique disk 1.9486/1.8433,
and mixed styles 0.7492/0.6805. The style probe improves about 9%; repeated disk
regresses about 4%. These are separate bounded row workloads, not retained
calamine equivalents. Unique RAM SST remains about 139 MiB RSS; other row probes
remain around 2 MiB.

Sampled SST disk peaks remain 16,128 and 126,000,000 bytes, with cleanup passing.
The error change does not alter model/SST budgets or storage strategies.

Raw samples include both scales:
[numeric](results/alpha7-compact-error-numeric.json),
[retained text/styles](results/alpha7-compact-error-models.json),
[row streams](results/alpha7-compact-error-streams.json).
Correctness, strict Clippy and Rust 1.88 loaded/streaming checks pass;
[ADR 0067](../docs/decisions/0067-compact-error-context.md) records the design.

Separate coarse CPU samples still identify shared XML event handling and cell
decoding as the dominant read costs. Attribute-allocation prototypes did not
establish consistent gains and were discarded. Further parser/binding work and
M4/M6 feature completion remain open.
