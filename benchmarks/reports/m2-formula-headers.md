# Compatible visible formula headers

probe_formula_headers.py uses public openpyxl 3.1.5 calls to verify four original source cases: unknown encoding type text, ordinary invalid unused flags/index/input hints, unknown ordinary attributes, and shared invalid unused hints. Formula text and numeric cache match native streaming verification. All attributes still validate XML/entities/individual byte limits; strict group and editor decoding retain the prior rejection behavior. Array/table raw flags and complete structured-header compatibility remain open.

Workspace tests, rustfmt, Clippy and release public interoperability pass. Results are m2-formula-header-interop.json and m2-formula-header-regression.json under results/. Original upstream selected assertions are untouched.

Five rotating serial samples plus one warmup compare current against the immediately preserved 5ecb5bd6e032294d7c18ae461befe61f86d88b79 binary (SHA256 b4e5012a9f9369bd621b59896f3610b9b4f930d401e1e20b2d765052e36c4010) and openpyxl. No builds/tests overlap timing. Both readers stream/verify all expanded expressions; native checks caches in the same timed pass, while public cache verification is outside timing. Cold process baseline is included.

| Cells | Current seconds / peak RSS KiB | Prior core seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- | --- |
| 20,000 | 0.032334 / 1,992 | 0.030004 / 2,012 | 0.365629 / 36,736 |
| 200,000 | 0.303352 / 1,976 | 0.296839 / 1,948 | 2.066373 / 44,540 |

The larger native case is 2.2% slower and 28 KiB higher RSS; this is a compatibility change, not a speed improvement. Native remains faster/lower RSS than openpyxl here. Sparse shared-template storage is 355 managed bytes independent of followers. No temporary storage is used and sampled peaks are zero. Native-competitor equivalence remains separate. Neither these readings nor narrow header support complete M2/M4.
