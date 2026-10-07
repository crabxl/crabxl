# A11 native point hyperlink checkpoint

Linux x86_64, Rust 1.99 release, default compression, three fresh serial processes
per size, no simultaneous builds/tests. Run `cargo build -p crabxl --example
hyperlinks --release --locked`, then `target/release/examples/hyperlinks 8000
/tmp/links.xlsx`. The example creates one external target/tooltip per row, saves,
drops the owned worksheet and explicitly scans resolved hyperlink declarations.
Scalar-cell materialization is not part of the scan phase.

| Rows | Median prepare (s) | Median save (s) | Median scan (s) | Median kernel peak RSS (KiB) | Peak managed temporary worksheet XML (bytes) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2,000 | 0.000812 | 0.006906 | 0.005700 | 3,488 | 385,622 |
| 8,000 | 0.003431 | 0.029616 | 0.022970 | 7,412 | 1,561,622 |

Raw samples and checksums: [alpha11-point-hyperlinks.txt](alpha11-point-hyperlinks.txt).
Kernel RSS comes from the worker's `/proc/self/status` VmHWM, avoiding inherited
Python parent high-water RSS. Output is a caller-owned file outside the reported
managed temporary XML. `finish` removes its owned worksheet spool. Physical disk
high-water and failure-path temporary space are not measured by this probe.

openpyxl 3.1.5 reopened both outputs and verified row extent and first/last cell
values, escaped URL fragments/targets and tooltips. The public Rust round-trip
also covers relative targets, location-only records and optional display. These are small native
checkpoint probes, without warmups or a competitor timing comparison; they do not
establish general read/write superiority or full A11 acceptance.
