# Canonical worksheet viewport checkpoint

Selected pinned rust_xlsxwriter pane/selection/view composition and XML layouts are integrated into shared Rust models and XLSX codecs, not a wrapped upstream workbook. Typed support covers baseline worksheet-view attributes, multiple source-ordered views/selections, frozen/split panes, defaults, sparse owned-sheet metadata/copy, bounded header reads, creation and repeated original-package editing. Chartsheet/workbook/custom views and unknown view extensions remain open. See ADR 0042 and third_party/ports.json.

Public openpyxl 3.1.5 independently verifies every view property, pane/selection order/default and numeric value. Native source header read plus streaming export is checked separately outside timing. Deterministic tests cover finite/XML validation, atomic allowance failure, structural recount/copy, Strict prefixed namespaces, mixed/repeated saves, pure-view formula-cache/calculation-chain preservation and cleanup. Workspace tests, rustfmt and Clippy pass.

Raw evidence: results/m5-worksheet-views.json and results/m5-worksheet-views-regression.json. One warmup plus five rotating serial Linux wait4 samples include runtime/import baseline; no builds/tests overlap timing. Creation uses sequential worksheet spools in both engines. Editing uses a bounded native original-package overlay versus an ordinary loaded reference workbook. Outputs have identical public views and scalar data, but ownership costs are explicitly different. Native creates no worksheet spool for editing. No calamine/rust_xlsxwriter comparison is claimed here: multiple views and arbitrary public attributes have not yet been mapped into an equivalent upstream public workload.

| Rows / operation | crabxl seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- |
| 5,000 / create | 0.005925 / 3,032 | 0.208979 / 34,912 |
| 50,000 / create | 0.043931 / 3,080 | 0.464501 / 34,912 |
| 5,000 / edit | 0.043539 / 3,172 | 0.232749 / 38,328 |
| 50,000 / edit | 0.427745 / 3,172 | 0.802613 / 72,568 |

Required faster-than-openpyxl and desired lower RSS hold for these supported workloads. The retained caller-owned two-view model estimate is 992 bytes, including actual spare vector capacities. The editor overlay estimate is 1,272 bytes including the conservative map node/part key. These managed estimates exclude allocator/dependency baseline and are not RSS measurements.

Exact native creation spool peaks are 237,675 / 2,517,677 bytes. Median 25ms samples are 0 / 2,517,677; short small-file allocation lifetimes are missed. Reference create samples are 205,197 / 2,817,840; edit samples 151,845 / 2,817,868, with no exact reference spool telemetry. Native edit exact/sampled temporary storage is zero. Final ZIP files are outside the private sampled TMPDIR; cleanup is checked after every process. Input archive storage, OS page cache and allocator overhead are not included in these temporary figures.

The immediate prior core is e2e5185453f4e4a3437a29872c38ce2dac39e8bf. Preserved write_demo binary SHA256: d7e3ebcce306528dab3bdf3b56cae835286a52726c0ac8141dfb6da1ce76495e. The unconfigured numeric fast path keeps its static header. Every current/prior worksheet XML SHA256 and byte count matches.

| Numeric cells | Current seconds / peak RSS KiB | Prior seconds / peak RSS KiB |
| --- | --- | --- |
| 50,000 | 0.042658 / 2,348 | 0.041918 / 2,372 |
| 500,000 | 0.443183 / 2,308 | 0.472958 / 2,412 |

The small case is about 1.8% slower and 24 KiB lower RSS; the larger case about 6.3% faster and 104 KiB lower RSS. Exact spool peaks are unchanged: 1,526,880 / 16,316,891 bytes. These are checkpoint trend measurements, not a claim of speed optimization or a stable timing guarantee.

Reproduce after compiling binaries, without builds/tests during timing:

```sh
cargo build --release --locked -p crabxl --example worksheet_views --example write_demo
python benchmarks/worksheet_views_checkpoint.py --runs 5
python benchmarks/worksheet_views_regression.py --baseline /path/to/prior/write_demo --runs 5
```

Header reads deliberately stop at the views container or sheetData and do not verify the remaining XML/CRC. Standard schema order is required. Unknown view extensions reject typed replacement but remain preservable with unchanged original-package saves. Integer fields use i64; nonfinite splits reject instead of reproducing invalid reference XML. This checkpoint does not complete M5, M4 loaded feature graphs or overall core acceptance.
