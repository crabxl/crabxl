# Canonical printing checkpoint

Selected pinned rust_xlsxwriter print-option, margin, page-setup and row/column-break XML layouts are integrated into canonical core models and bounded XLSX codecs. Typed support covers baseline field names, optional values/false flags, paper dimensions, pageSetUpPr, sparse break records and literal printer IDs. Read/create/owned-copy/original-package-edit paths share these models. New/changed printer graphs, full sheet properties, header/footer creation/editing, print areas/titles and protection remain open. See ADR 0043 and third_party/ports.json.

Public openpyxl 3.1.5 verifies all printing properties, break order/defaults/counts and every numeric value. Native full-source metadata read plus streaming export is independently checked outside timing. Nullable break coordinates and literal none page enum tokens follow public defaults. Deterministic tests also cover combined views/printing, aggregate/copy/recount budgets, finite/XML failures, bounded footer admission, repeat/mixed save, source properties/header-footer schema ordering, Strict prefixed namespace context, printer identity/binary preservation and cleanup. Workspace tests, rustfmt and Clippy pass.

Raw evidence: results/m5-printing.json and results/m5-printing-regression.json. One warmup plus five rotating serial Linux wait4 samples include runtime/import baseline. Builds/tests do not overlap timing. Creation uses sequential worksheet spools in both engines. Editing uses a native bounded original-package overlay versus an ordinary loaded reference workbook; output properties/data match, but ownership costs differ. Source printing validation scans/decompresses the worksheet to EOF/CRC, and native saving streams it again. Cells/worksheet XML are not materialized. Calamine/rust_xlsxwriter overlap measurements remain open: this public literal/optional printing workload has not yet been mapped to equivalent upstream calls.

| Rows / operation | crabxl seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- |
| 5,000 / create | 0.005929 / 2,968 | 0.225398 / 35,924 |
| 5,000 / edit | 0.049849 / 3,060 | 0.271096 / 39,052 |
| 50,000 / create | 0.046648 / 2,976 | 0.503416 / 35,924 |
| 50,000 / edit | 0.479907 / 3,064 | 0.802659 / 73,448 |

Required faster-than-openpyxl and desired lower RSS hold for these supported workloads. Caller-owned settings retain a managed estimate of 593 bytes, including actual spare break vector capacities and literals. The editor overlay estimate is 1,641 bytes including its conservative map node/part key. These estimates exclude allocator/dependency baseline and are not RSS figures. Metadata/header/footer encoding uses separate bounded buffers; only the footer remains in the active writer and participates in metadata/temp/part admission.

Exact native creation spool peaks are 237,698 / 2,517,700 bytes; median 25ms samples 0 / 2,517,700. Short small-file allocation lifetimes are missed. Reference creation samples are 110,801 / 2,817,852; editing samples 160,040 / 2,817,880, with no exact reference spool telemetry. Native editing adds no worksheet spool: exact/sampled temporary storage is zero. Final ZIP files are outside private sampled TMPDIR, and cleanup is checked after each process. Original archive space, OS file cache and allocator overhead are additional costs.

The immediate prior core is b3e3460408bf2a034a7826eae35b7a0321f399ce. Preserved write_demo binary SHA256: 0100ac1dd42b8db9390327e7f750c012b920dcb17b16d609dd8047e1953c878a. Default numeric sheets retain static headers/footers, and every current/prior worksheet XML SHA256/byte count matches.

| Numeric cells | Current seconds / peak RSS KiB | Prior seconds / peak RSS KiB |
| --- | --- | --- |
| 50,000 | 0.043788 / 2,352 | 0.044071 / 2,372 |
| 500,000 | 0.464827 / 2,356 | 0.464036 / 2,312 |

The small case is about 0.6% faster and 20 KiB lower RSS. The larger case is about 0.2% slower and 44 KiB higher RSS. Exact spool peaks are unchanged at 1,526,880 / 16,316,891 bytes. No speed/RAM optimization is claimed.

Reproduce after compiling binaries, without builds/tests during timing:

```sh
cargo build --release --locked -p crabxl --example printing --example write_demo
python benchmarks/printing_checkpoint.py --runs 5
python benchmarks/worksheet_views_regression.py --baseline /path/to/prior/write_demo --baseline-core b3e3460408bf2a034a7826eae35b7a0321f399ce --output benchmarks/results/m5-printing-regression.json --runs 5
```

Retained printer IDs must equal their original identities, with original printer binary/relationships copied unchanged. New links, changed IDs, signed packages and unknown printing extensions reject modification. Pre-existing dangling printer targets are not repaired. Pure printing/view edits retain caches/chains; value/formula changes still trigger global recalculation policy. Integers use i64, nonfinite margins reject, and arbitrary invalid-schema descriptor coercions remain incomplete. This checkpoint does not complete M5.7, M5, M4 loaded feature graphs or overall Rust core acceptance.
