# Alpha.5 explicit theme catalog scaling

Linux x86_64, Rust 1.99 release build. One warmup and three rotating serial native
samples include archive opening, opaque theme preparation and explicit typed
palette/font parsing. Fixture generation, compilation, tests and CRC readback
are excluded. Inputs retain the same worksheet package; no worksheet cells are
decoded. This is scaling evidence, not comparison with another library.

| Supplemental fonts | Theme XML bytes | Median seconds | Peak RSS KiB | Typed catalog bytes |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 183 | 0.001142 | 1,488 | 729 |
| 1,000 | 41,963 | 0.001909 | 1,616 | 44,277 |
| 100,000 | 4,577,963 | 0.093666 | 15,260 | 4,678,509 |

Every sample verifies the returned record count. Fixtures are read through ZIP
EOF for CRC verification and theme payload hashes are retained. Exact palette,
font properties, namespace and source preservation assertions live in the
integration tests; count checks alone do not prove those semantics.

There is no temporary string store. Typed catalogs and source theme bytes are
intentionally owned; their memory grows with requested font records and is
bounded by configured byte/record allowances. Peak RSS includes runtime,
archive/dependency allocations and parsing transients, and is not equivalent to
the reported catalog ledger. Raw samples and executable/source hashes are in
[results](results/alpha5-theme-catalog.json).

```sh
cargo build --release -p crabxl --example theme_read --locked
python benchmarks/theme_catalog.py --binary target/release/examples/theme_read \
  --measure /path/to/measure --source /path/to/workbook-with-default-theme.xlsx \
  --output /tmp/theme-catalog.json
```
