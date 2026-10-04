# M2 ISO dates and date-only values

The canonical core now exposes date-only literals and ISO parsing/formatting. Imported `d` cells and formula caches preserve date/calendar/clock/duration kinds. Optional ISO creation writes calendar/clock cells independent of workbook serial epoch; elapsed durations remain numeric. M2 is not complete.

Reproduce with `CARGO_TARGET_DIR=<target> python benchmarks/iso_checkpoint.py`. Four columns repeat an early Gregorian date, microsecond datetime, microsecond clock and elapsed duration. Every output from every creation sample is verified through public openpyxl 3.1.5 value/type APIs, outside creation timing. Native/public read workers verify every cell on the same native-created input. Windows and Mac epochs are separate cases. One warmup/five rotating serial runs measure Linux process wall/CPU/wait4 peak RSS, excluding builds/generation and including runtime baseline. The host remains limited to two CPU quota units and 8 GiB. Both writers use sequential worksheet spools, and both readers stream; no entire native worksheet/XML is retained in RAM.

| Cells | Epoch | Operation | Engine | Seconds | CPU seconds | RSS MiB |
| ---: | --- | --- | --- | ---: | ---: | ---: |
| 40,000 | win | create | crabxl | 0.0412 | 0.0410 | 2.10 |
| 40,000 | win | create | openpyxl | 0.5120 | 0.6924 | 32.46 |
| 40,000 | win | read | crabxl | 0.0443 | 0.0442 | 1.92 |
| 40,000 | win | read | openpyxl | 0.4409 | 0.6011 | 33.85 |
| 40,000 | mac | create | crabxl | 0.0402 | 0.0401 | 2.10 |
| 40,000 | mac | create | openpyxl | 0.4984 | 0.6856 | 32.46 |
| 40,000 | mac | read | crabxl | 0.0423 | 0.0422 | 1.88 |
| 40,000 | mac | read | openpyxl | 0.4351 | 0.6210 | 33.84 |
| 400,000 | win | create | crabxl | 0.3788 | 0.3780 | 2.06 |
| 400,000 | win | create | openpyxl | 3.5891 | 3.7484 | 32.46 |
| 400,000 | win | read | crabxl | 0.4232 | 0.4231 | 1.88 |
| 400,000 | win | read | openpyxl | 2.9397 | 3.1280 | 41.25 |
| 400,000 | mac | create | crabxl | 0.3811 | 0.3802 | 2.09 |
| 400,000 | mac | create | openpyxl | 3.6371 | 3.8201 | 32.45 |
| 400,000 | mac | read | crabxl | 0.4184 | 0.4173 | 1.89 |
| 400,000 | mac | read | openpyxl | 3.1105 | 3.2465 | 41.25 |

Native speed exceeds openpyxl for these equivalent creation/read outputs and RSS is lower. This does not establish whole-baseline acceptance. calamine and rust_xlsxwriter are not compared until overlapping typed ISO behavior is validated; absent comparisons are not wins. Full samples, output/XML sizes and temporary peaks are in [m2-iso-dates.json](results/m2-iso-dates.json).

Native creation reports exact logical temporary bytes after closing the worksheet, including its footer. The benchmark asserts equality to the completed ZIP worksheet XML byte size. Shared TMPDIR disk sampling every 25ms is a lower bound, excludes target ZIP and verifies no residual files after each process. At 100,000 rows native worksheet temp is 21,144,642 bytes (20.17 MiB); public reference worksheet XML is 21,744,911 bytes (20.74 MiB). Reading uses no temporary worksheet files. Output ZIP space is separate; all sample sizes are recorded. Low RSS is not zero disk/I/O cost.

## Public utility behavior and limits

`python benchmarks/probe_iso_dates.py docs/research/iso-date-public-probe.json` records 42 public utility observations without inspecting implementation. The native `iso_probe` example matches every successful value/kind/ISO representation and rejection, recorded in [m2-iso-public-interop.json](results/m2-iso-public-interop.json). Compatibility includes recognized prefix acceptance, ignored zone/trailing suffixes, three-digit fraction truncation and positive hour/minute/second elapsed notation. It is not strict ISO or timezone-aware validation. Empty values are absent; nonempty ISO values are not whitespace-trimmed.

Generated deterministic tests cover both epochs, read/materialized parity, formula caches, malformed coordinates/context, unsupported prefixes, integral date-only raw serials, byte limits before spooling, valid retry and cleanup. The extra date-only raw-serial validation was added after timing; measured valid ISO paths use literal constructors and are unchanged. Default numeric output remains available and can lose literal precision during baseline numeric loading. Themes, full imported style editing, shared/array/data-table formulas and aggregate catalog accounting remain open.
