# M3 sequential writer acceptance and resource evidence

The roadmap M3 creation scope is verified: exact scalars/plain inline text, date/time/duration in both epochs, normal formulas with optional typed caches and basic shared styles. This does not complete M2 date/style/shared-string reading or M4 existing-file editing/preservation.

Measurements use Rust 1.88.0 release builds and openpyxl 3.1.5 on this Linux workspace. Each Rust case has one warmup and three measured runs. The table reports median wall/CPU/peak RSS; logical temporary XML and output sizes are deterministic within each case. Generation, validation and public Python readback are outside timed processes. Numeric Python write-only results have only one measured run (a warmup at 10k/100k, none at 1m), so ratios are local observations with limited comparison confidence. Native wait4 measures RSS, wall and CPU; temporary files are sampled every 25 ms, and Rust also reports exact logical bytes including buffers.

| Workload | Rows × columns | Wall s | CPU s | Peak RSS MiB | Temporary XML MiB | XLSX MiB |
|---|---:|---:|---:|---:|---:|---:|
| Rust numeric | 10,000 × 10 | 0.0827 | 0.0826 | 2.09 | 2.93 | 0.29 |
| Python numeric | 10,000 × 10 | 0.8504 | 0.6054 | 32.50 | ≥3.51 | 0.31 |
| Rust numeric | 100,000 × 10 | 0.8863 | 0.8861 | 2.06 | 31.34 | 2.81 |
| Python numeric | 100,000 × 10 | 3.9985 | 4.1688 | 32.42 | ≥37.07 | 3.00 |
| Rust numeric | 1,000,000 × 10 | 7.6793 | 7.6773 | 2.07 | 333.47 | 29.24 |
| Python numeric | 1,000,000 × 10 | 38.9337 | 39.0383 | 32.36 | ≥390.69 | 30.12 |
| Rust mixed | 10,000 × 10 | 0.0603 | 0.0602 | 2.10 | 4.53 | 0.36 |
| Rust mixed | 100,000 × 10 | 0.6149 | 0.6147 | 2.11 | 46.50 | 3.46 |
| Rust features | 10,000 × 10 | 0.0615 | 0.0614 | 2.04 | 4.49 | 0.31 |
| Rust features | 100,000 × 10 | 0.6324 | 0.6323 | 2.03 | 45.95 | 3.12 |
| Rust features1904 | 10,000 × 10 | 0.0623 | 0.0622 | 2.03 | 4.49 | 0.31 |
| Rust features1904 | 100,000 × 10 | 0.6333 | 0.6331 | 1.98 | 45.95 | 3.12 |

The one-million-row numeric case spools approximately 333.47 MiB before packaging. Low process RSS excludes OS page cache and any tmpfs-backed storage: temporary disk traffic and system memory remain real costs. Filesystem choice matters; temp directory, file buffer, encoded row/cell limits, catalog/style metadata/count and total/per-sheet temporary limits are configurable. Caller rows/output, allocator overhead and all dependency allocations are not included in a hard RSS ceiling.

Verification covers numeric checksums for Rust and Python output through 1m rows; public Python numeric readback through 100k; exact integer/float/bool/error/whitespace/empty mixed values through 100k. Feature workloads verify every cell in normal and data-only modes through 100k rows in Windows and Mac epochs, including missing versus zero/false/string/error caches, calendar milliseconds, time, duration and font/fill/border/alignment/format/protection attributes. ZIP CRC, internal relationship targets/unique IDs, style collection counts and absent-cache XML are checked. Temporary directories must be empty after each run. This is generated interoperability evidence, not Excel UI validation or complete M5 style support.

There are 61 passing deterministic tests including the facade doc test, plus warning-free Clippy and API documentation. Failure tests cover bad rows/style IDs/formulas/formats, byte/count budgets, late-row state, next-temp-file creation, partial output, individual unlink failure/retry, explicit abort and abandonment. Raw data: [m3-writer.json](results/m3-writer.json).

## Reader regression

The shared StyleId increases Cell storage from 24 to 32 bytes while CellValue stays 16 bytes. Scalar/formula heap payloads remain separately accounted in row/batch/materialization/Auto budgets. The reader also gains normal formula caches and XML newline normalization. A warmup and three alternating numeric runs compare the new reader with the fixed M1 `74c082a58e8b` reader; values/checksums must agree. Raw data: [m3-scalar-regression.json](results/m3-scalar-regression.json).

| Read workload | Rows × columns | Version | Wall s | CPU s | RSS MiB |
|---|---:|---|---:|---:|---:|
| mixed | 10,000 × 10 | M3 | 0.0646 | 0.0644 | 1.42 |
| mixed | 100,000 × 10 | M3 | 0.6517 | 0.6516 | 1.56 |
| numeric | 100,000 × 10 | M1 | 0.6986 | 0.6980 | 1.57 |
| numeric | 100,000 × 10 | M3 | 0.7307 | 0.7302 | 1.59 |
| numeric | 1,000,000 × 10 | M1 | 7.0518 | 7.0508 | 1.58 |
| numeric | 1,000,000 × 10 | M3 | 7.1217 | 7.1172 | 1.61 |

Numeric wall time is about 4.6% higher at 100k and 1.0% higher at 1m in this three-run probe; these changes do not establish a universal regression or speedup. Streaming RSS remains bounded at fixed row/input settings. Materialized cells do incur the additional style-identity bytes; Auto estimates and enforced retained-data budgets account for the new size. Reader runtime temporary storage is zero for these workloads. Mixed files are checked using public openpyxl values/types.

## Reproduction

Require Linux, `cc`, Rust 1.88.0 and `openpyxl==3.1.5`:

```sh
cc -O2 benchmarks/measure.c -o benchmarks/measure
cargo build --release --examples --locked
python benchmarks/writer_checkpoint.py
git worktree add /tmp/crabxl-m1 74c082a58e8b
CARGO_TARGET_DIR=/tmp/crabxl-m1-target cargo build --release --manifest-path /tmp/crabxl-m1/Cargo.toml --example sum
python benchmarks/write_baseline.py benchmarks/data/numbers-100000.xlsx 100000
python benchmarks/write_baseline.py benchmarks/data/numbers-1000000.xlsx 1000000
python benchmarks/scalar_checkpoint.py --numeric-before /tmp/crabxl-m1-target/release/examples/sum --after-label M3 --output benchmarks/results/m3-scalar-regression.json
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Generated inputs, native launcher and Cargo targets are ignored. Timing thresholds are not correctness assertions.
