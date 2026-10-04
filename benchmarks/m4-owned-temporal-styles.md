# Owned temporal assignment and streaming regression

The owned native engine retains all cells, resolves final style IDs during assignment and consumes its model/catalog on export. Public openpyxl 3.1.5 uses ordinary cells with source format set before value assignment. Every saved value/type/format/font/fill/border/alignment is verified outside timing. Deterministic core tests additionally verify the pre-save styles, sharing and failure behavior. No earlier native owned API with this automatic pre-save behavior is fabricated as a feature baseline.

Run owned_temporal_checkpoint.py after a release temporal_style_fixture build. Raw evidence: results/m4-owned-temporal-styles.json. One warmup plus five rotating serial cold-process wall/CPU/RSS samples include runtime/import baseline. No tests/builds overlap timing. Final ZIP is outside monitored temporary space, and every run checks temporary cleanup.

| Owned cells | crabxl seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- |
| 5,000 | 0.007698 / 2,692 | 0.284893 / 36,688 |
| 50,000 | 0.059873 / 8,180 | 1.378040 / 57,512 |

The required speed and desired RSS targets hold for this overlap. Conservative managed bank accounting is 1,486,344/14,806,344 bytes before transfer, including a 256-byte node allowance plus owned temporal payload per cell; it is not an RSS claim. Final writer style storage is constant at 6,323 managed bytes. Exact worksheet spools are 239,775/2,456,031 bytes; native sampled medians are zero/2,456,031 and public 131,951/2,693,828. The smaller zero is a missed short spool, not no temporary I/O.

Streaming regression compares the preserved 26f4f5d7d18aaf36a2669174d925e71041f51159 binary, SHA256 0d17a5a3888426f3047852fe0a41bdcf1502c02586bae35abb7a77b051526792. That version predates the indexed-palette checkpoint; the fixture has no explicit indexed palette. Streaming and owned results are separate because they retain different models.

| Streaming cells | Current seconds / RSS KiB | Prior seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 5,000 | 0.007071 / 2,420 | 0.007228 / 2,384 | 0.301848 / 34,768 |
| 50,000 | 0.049277 / 2,432 | 0.048124 / 2,380 | 1.445212 / 34,764 |

The larger native result is 2.4% slower and 52 KiB higher RSS; this is a compatibility checkpoint, not an optimization claim. Exact native spools remain 241,275/2,471,031 bytes. Sampled current medians are zero/2,092,439, prior zero/2,190,548 and public 234,952/2,693,800. Sampling every 25 ms can miss peaks. calamine creation and equivalent rust_xlsxwriter comparisons are not claimed. All remaining milestone acceptance remains visible.
