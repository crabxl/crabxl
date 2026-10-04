# M2 exact scalar and owned-payload checkpoint

Rust 1.88.0, openpyxl 3.1.5, native Linux wait4 RSS; three measured runs after one warm-up. Numeric before/after runs alternate. Raw results: [results/m2-scalars.json](results/m2-scalars.json).

| Ten-column workload | Median seconds | Median peak RSS MiB |
|---|---:|---:|
| Mixed values, 10,000 rows | 0.0655 | 1.54 |
| Mixed values, 100,000 rows | 0.6394 | 1.61 |
| Numeric, 100,000 rows, M1 | 0.7162 | 1.51 |
| Numeric, 100,000 rows, M2 | 0.6921 | 1.57 |
| Numeric, 1,000,000 rows, M1 | 7.0690 | 1.58 |
| Numeric, 1,000,000 rows, M2 | 6.8018 | 1.51 |

The mixed fixture contains an integer beyond 2^53, an integer beyond i64, a floating-point literal, a boolean, an error, repeated whitespace-bearing text, unique text, an empty inline string, a physically empty cell, and a row-number integer. Public openpyxl read-only results are checked for every row, including integer/float/boolean types. The Rust example asserts scalar counts and true counts for each run; exact literal contents and ownership/budgets are covered by integration tests. Fixture XML is generated incrementally using openpyxl-created package metadata, not copied upstream binaries.

Numeric counts/checksums remain identical. The current checkpoint is about 4% faster than the interleaved M1 numeric baseline in this run; the previous boolean checkpoint's numeric regression is absent here. Three runs do not establish a universal speedup. No runtime temporary files are used. High-cardinality inline text is streamed; shared-string indexing, rich strings, styles/dates/formulas and OOXML text-escape interpretation remain incomplete M2 requirements.

Common CellValue storage remains 16 bytes. i64 and booleans need no heap payload; large integers/text/errors use indirection. Owned wrapper and UTF-8 byte allocations count toward rows, batches, explicit materialization, and Auto sampling/fallback. Parser buffers, allocator overhead, and caller-retained output are separate; these are not hard process RSS guarantees. Payload bytes are accumulated once per row, avoiding repeated scans of prior cells during decoding.

```sh
cargo build --release --examples --locked
python benchmarks/scalar_checkpoint.py --numeric-before /path/to/m1-sum
```

Build the M1 sum executable at `74c082a58e8b` separately; compile benchmarks/measure.c and generate the existing numeric inputs as described in README.md. Fixture generation and Python correctness verification are excluded from Rust measurements. The sum example rejects integer inputs beyond 2^53 instead of silently losing precision; the controlled numeric benchmark falls within that range.
