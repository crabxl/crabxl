# A8 byte-oriented XML escaping

Compare core `4f25c545684a03a879d717c27cf1bb91acd14676` with
[ADR 0079](../docs/decisions/0079-byte-oriented-xml-escaping.md). Reports include
exact encoder patches, identical comparison-worker source, binary hashes,
individual samples and output checks:
[text writes](results/alpha8-byte-escape-text-writes.json) and
[numeric writes/edits](results/alpha8-byte-escape-numeric.json).

One warmup and three rotating native release repetitions ran serially without
overlapping builds, tests, profiling or verification. Text generation and the
complete build/save operation are timed. Independent openpyxl 3.1.5 readback
checks every coordinate/value; all uncompressed package parts match before/after
within each creation mode. Numeric readback and edit checks use the existing
comparison contract. Owned models use 1 GiB allowances; writer/compression
settings remain identical. RSS and temporary descriptor sampling are separate.

| Large workload | Previous seconds | Candidate seconds |
| --- | ---: | ---: |
| One million Unicode/XML text cells, streaming creation | 1.222869 | 1.154405 |
| One million Unicode/XML text cells, owned model creation | 1.412204 | 1.367243 |
| One million numeric cells, streaming creation | 0.869679 | 0.892146 |
| One million numeric cells, owned model creation | 0.943265 | 0.963149 |
| Two million numeric cells, load/edit/save | 2.984829 | 2.981215 |

Large text streaming median RSS is 5,252 KiB; owned text creation is 209,116 KiB.
Both sample a roughly 255 MB temporary worksheet/output peak, comparable to the
previous encoder. Output archives remain 4,396,092 bytes. No RAM or temporary
storage reduction is claimed. Small text stream samples are slower; numeric
changes are mixed and the small edit difference is not meaningful.

rust_xlsxwriter 0.99.1 normal/constant numeric references remain in the raw
report. The unchanged normal reference varies substantially between the two
linked workers, limiting conclusions from small timing differences. Candidate
owned numeric creation is roughly comparable in elapsed time and uses much less
RSS; a stable all-workload speed win remains unestablished. Text carriage-return
compatibility and differing string-storage policies are not equated to reference
writer modes in this report.

This is a narrow text-encoding improvement. Conservative model accounting,
large real-world fixtures, Python write conversion and wider write/edit targets
remain tracked independently from these measurements.
