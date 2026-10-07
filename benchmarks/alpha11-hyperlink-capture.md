# A11 streamed hyperlink capture checkpoint

Linux x86_64, Rust 1.99 release, default compression. Three fresh serial processes
per size/mode, no overlapping builds/tests, no warmup. The generator writes one
external target and tooltip per row, then drops the original sheet before reading.

Run `cargo build -p crabxl --example hyperlinks --release --locked`, then
`target/release/examples/hyperlinks 8000 /tmp/links.xlsx combined` (or `scan`,
`loaded`). `scan` decodes metadata only; `combined` consumes/checks all scalar cells
and captures declarations in the same worksheet scan before relationship
resolution; `loaded` explicitly materializes cells and borrows canonical links.
The loaded measurement does not clone the collection.

| Rows | Read mode | Median read/check phase (s) | Median kernel peak RSS (KiB) |
| --- | --- | ---: | ---: |
| 2,000 | metadata scan | 0.005949 | 4,116 |
| 2,000 | combined row/capture/resolve | 0.007195 | 4,184 |
| 2,000 | loaded model | 0.008500 | 4,520 |
| 8,000 | metadata scan | 0.025003 | 8,068 |
| 8,000 | combined row/capture/resolve | 0.029539 | 8,220 |
| 8,000 | loaded model | 0.030630 | 9,200 |

[Raw samples](alpha11-hyperlink-capture.txt) include preparation/save times and
matching checksums 2,087,890 / 32,354,890. Peak managed temporary worksheet XML is
385,622 / 1,561,622 bytes; owned spools are removed by finish. Output files are
caller-owned and excluded from these temporary XML figures. Physical disk high-water
and failure-path temporary space are not measured. Kernel VmHWM comes from the
worker's `/proc/self/status`, not the orchestration parent's inherited RSS.

These modes have different behavior and are not interchangeable performance
comparisons. Source construction/SST/style metadata and checksum validation are
included in the read phase. Peak RSS covers the complete generator/reader process.
No competitor timing, general speed superiority, final A11 acceptance or hard RSS
ceiling is claimed. The earlier point probe remains separate evidence for its
previous revision; this checkpoint adds aggregate/decoded relationship charging.
