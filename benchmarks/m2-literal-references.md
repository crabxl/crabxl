# Literal formula reference checkpoint

Public openpyxl 3.1.5 constructors and serializers generate the comparison records; no reference implementation is inspected. probe_literal_references.py verifies matching formula XML, ordinary property readback and absent data-only caches for five array reference strings and four data-table input strings. Inputs include empty strings, absolute A1, worksheet-qualified and opaque values. Assigned empty array ref remains empty before save, is omitted by Compatible output and reloads as None. RetainExplicit retains the attribute. The core owns literal strings without eager geometry parsing; explicit range/input checks remain available.

Raw results: results/m2-literal-references-interop.json and results/m2-literal-references-regression.json. Workspace tests, rustfmt, Clippy and release interoperability pass. No milestone completion is implied.

The Linux Rust 1.88/Python 3.12 regression workload reads ordinary/shared formulas in streaming mode, checks every expanded expression, and runs one warmup plus three rotating serial samples. Native also checks every cache in its timed pass; openpyxl verifies caches in a separate untimed pass. Process/import baseline is included. No builds/tests overlap timed samples. The preserved 93951397199bde14f75d6d66da81b6fc43dcbf36 binary predates several aggregate/formula checkpoints; this measures a trend, not an isolated effect of this change.

| Cells | Current seconds / peak RSS KiB | Preserved core seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- | --- |
| 20,000 | 0.030302 / 1,968 | 0.029972 / 2,064 | 0.333561 / 36,736 |
| 200,000 | 0.299216 / 1,972 | 0.287143 / 2,072 | 2.016850 / 44,672 |

The larger case is 4.2% slower than the preserved native baseline; no speed optimization is claimed. Native remains faster/lower RSS than openpyxl for this workload. One shared template accounts for 355 managed bytes, independent of follower count. Temporary storage is unused; sampled peak is zero for all readers. This workload does not establish calamine parity or writer performance. Literal-reference correctness is verified separately from timings.
