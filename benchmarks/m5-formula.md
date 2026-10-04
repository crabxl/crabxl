# M5 A1 translation checkpoint

This is a bounded A1 scanner/translated-model-move checkpoint, not complete M5 tokenizer/common-feature acceptance. CPython 3.12, openpyxl 3.1.5, PyO3 0.26 and Rust 1.88 release/thin LTO. Both engines receive the same public Translator expression/origin/destination; cases either construct a Translator for each call or reuse one. A fixed expression includes a normal range, quoted sheet name, absolute axis, structured table reference, function-like reference token and quoted literal.

Native wait4 records CPU/wall/peak RSS, including interpreter/conversion/import costs. One warmup and three alternating runs per engine. Length totals and SHA-256 of the final output are verified. Results are discarded rather than retained; translation uses no temporary storage.

| Calls | Mode | Adapter median | openpyxl median | Adapter RSS | openpyxl RSS |
|---|---|---:|---:|---:|---:|
| 10,000 | construct | 0.036 s | 0.495 s | 10.52 MiB | 31.96 MiB |
| 10,000 | reuse | 0.036 s | 0.253 s | 10.56 MiB | 31.84 MiB |
| 100,000 | construct | 0.146 s | 3.320 s | 10.59 MiB | 31.96 MiB |
| 100,000 | reuse | 0.121 s | 0.856 s | 10.58 MiB | 31.96 MiB |

The Rust scanner avoids a resident token model; it does not provide the complete tokenizer API. Creation/reuse comparison therefore applies only to the verified translation result. Openpyxl startup/import costs remain visible, especially for smaller cases.

The shared column formatter also affects numeric writer addresses. Same-call creation regression versus the preserved 1ad552d adapter measured:

- 10,000 rows by ten columns: 0.150 s before, 0.152 s current (+1.6%). Checksums and cleanup pass.
- 100,000 rows by ten columns: 1.398 s before, 1.355 s current (-3.1%). Checksums and cleanup pass.

Writer RSS stays about 18/86 MiB at 10k/100k rows; observed temporary peaks remain about 3.2/34.1 MiB (worksheet spools plus adjacent output ZIP). Output sizes/checksums are in raw results. Temp polling every 10 ms may miss short peaks. This experiment establishes no consistent writer speed improvement and no change to the owning-model/disk tradeoff.

Coverage includes 46 pinned original worksheet/translator methods, shared public-API cases, core quote/bracket/reference/bounds/output-budget tests and translated-move memory/bounds failure atomicity. Complete tokenizer APIs, dynamic spill syntax, arbitrary-size row labels and malformed-input exception equivalence remain staged. Negative/out-of-range offsets do not silently preserve stale original references. No Excel calculation engine is claimed.

[Raw runs](results/m5-formula.json). Reproduce with the installed release wheel and `python benchmarks/formula_checkpoint.py --before-python-path /path/to/1ad552d-package-snapshot`. The snapshot contains a preserved installed openrsxl package; the benchmark imports it via PYTHONPATH only for before-writer cases.
