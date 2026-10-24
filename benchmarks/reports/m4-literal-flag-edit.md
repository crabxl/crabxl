# Known literal flag replacement

This narrow original-package edit replaces A1, a known data-table formula with five opaque flag strings, with -1 and saves. Public openpyxl verifies every remaining reference/flag and the replacement after every process. Workspace tests also cover literal flag updates/repeated saves, escaped source strings, array hints, unaffected scalar cells/sheets and retained unknown/shared replacement guards. No original reference implementation is inspected.

Native uses lazy original-package editing; openpyxl loads an explicit full owned model. This compares the same operation and supported outputs with different memory modes, not identical materialization costs or complete feature graphs. One warmup plus five rotating serial cold-process measurements include process/import baseline; generation and public readback are excluded. No builds/tests overlap samples. Raw results: results/m4-literal-flag-edit.json.

| Source cells | crabxl seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- |
| 5,000 | 0.033995 / 2,652 | 0.268391 / 39,240 |
| 50,000 | 0.323896 / 2,656 | 1.136586 / 80,912 |

Speed/RSS goals hold for this supported edit with the stated mode difference. There is no equivalent prior known-opaque-flag edit API benchmark; the previous strict decoder rejects it. Native-competitor edit equivalence remains unestablished.

Worksheet temporary storage is sampled every 25 ms with cleanup checked: native medians zero and public medians 424,293/6,393,736 bytes. Native streams rewritten worksheet XML into ZIP and has no intermediate worksheet spool here. Final ZIP and native atomic ZIP staging are outside monitored TMPDIR and excluded from these values; saving still requires output disk space. Output byte sizes are recorded per sample. Sampling can miss short public spool peaks, and native zero must not be described as no disk I/O. Full M4 graph/structural/lifecycle acceptance remains open.
