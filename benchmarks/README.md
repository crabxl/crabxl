# Numeric read experiments

These experiments measure the first raw numeric checkpoint, not full spreadsheet compatibility. Inputs are generated with openpyxl 3.1.5 write-only: one sheet, ten columns, consecutive integers starting at zero. Every warmup and measured run verifies count and checksum. The largest checksum is 49,999,995,000,000, within exact f64 integer precision.

Rust 1.88.0 release builds use thin LTO. Baselines are openpyxl 3.1.5 read-only/data-only and crates.io calamine 0.36.1 materializing its worksheet Range. Python is 3.12.14 on Linux x86_64, AMD EPYC 9V74. Foundational dependency versions are locked; calamine is a separate benchmark workspace, never a core runtime dependency. Details and raw samples are in [numeric-results.json](numeric-results.json).

Wall time includes process startup, imports, package discovery, reading, summing, and cleanup. Generation and builds are excluded. Each scale has one warmup and five serial measured runs with rotating implementation order and warm filesystem cache. Peak RSS is the kernel per-process high-water mark from a native Linux fork/exec/wait4 launcher, without baseline subtraction. The native launcher avoids carrying a Python measurement harness's larger startup high-water mark into tiny Rust processes. There is no read-time temporary storage; generation may spool temporary XML and is excluded.

| Rows × columns | crabxl time / peak RSS | openpyxl time / peak RSS | calamine time / peak RSS |
|---|---|---|---|
| 10,000 × 10 | 0.072 s / 1.52 MiB | 0.538 s / 33.80 MiB | 0.032 s / 8.54 MiB |
| 100,000 × 10 | 0.713 s / 1.54 MiB | 4.382 s / 41.68 MiB | 0.316 s / 70.42 MiB |
| 1,000,000 × 10 | 6.943 s / 1.50 MiB | 56.569 s / 119.25 MiB | 3.103 s / 688.43 MiB |

Time and RSS are independent medians. Numeric streaming memory remains approximately flat as rows increase. At the largest scale crabxl is about 8.1 times faster than openpyxl and 2.2 times slower than calamine. Different validation, models, and execution paths mean this is not proof that streaming alone explains the speed difference. Bindings, text, styles, formulas, editing, and preservation are not measured.

## Input buffer tuning

The same ten-million-cell input was measured with one warmup per size and three serial runs in rotating order. [buffer-results.json](buffer-results.json) also records CPU time.

| Input buffer | Median wall time | Median CPU time | Median peak RSS |
|---|---|---|---|
| 32 KiB | 7.106 s | 7.106 s | 1.46 MiB |
| 256 KiB | 7.245 s | 7.238 s | 1.50 MiB |
| 1 MiB | 7.153 s | 7.147 s | 2.40 MiB |
| 8 MiB | 6.941 s | 6.940 s | 9.41 MiB |

This path is primarily CPU-bound on this host: CPU and wall time are close. The largest buffer's roughly 2% median difference is small compared with sample variation; do not infer a reliable speedup from three runs. Adding RAM to this buffer is not enough to solve the throughput gap. Larger caches, compact layouts, indexes, parallel work, and parser improvements need separate profiling/measurement. The initial numeric Auto policy below uses measured useful materialization for repeated access; further cache/concurrency strategies remain in [ADR 0002](../docs/decisions/0002-adaptive-memory.md).

## Streaming versus owned sheet materialization

The non-streaming public operation `read_sheet()` uses the same incremental parser and retains all rows. [mode-results.json](mode-results.json) measures load plus one sum, with a 32 KiB input buffer, a 1 GiB retained-data budget, one warmup per mode, and three alternating runs.

| Mode | Median wall time | Median peak RSS |
|---|---|---|
| Reused streaming row | 6.971 s | 1.51 MiB |
| Owned materialized sheet | 7.163 s | 413.35 MiB |

Materialization does not accelerate the first scan here. Its benefit is retaining data for later access without rereading ZIP/XML; repeated-query performance is not measured in this experiment. It is a numeric snapshot, not the future editable workbook model. The specified budget covers retained row/vector capacity, not total process RSS; parser/catalog memory and one current row are additional.

## Adaptive numeric reads

[adaptive-results.json](adaptive-results.json) records one warmup and three rotating serial runs on the same input. A repeated-access workload sums the ten-million-cell sheet three times, including initial load. Every pass contributes to the verified count/checksum. Default Auto discovered approximately 6.0 GiB effective availability under an 8 GiB cgroup limit, then derived an operation budget of about 1.44 GiB after headroom/fraction controls. It estimated 831,593,274 retained bytes and selected materialization; actual RSS was lower. The 64 MiB explicit policy selected streaming before materialization.

| Policy / workload | Selected mode | Median wall time | Median peak RSS |
|---|---|---|---|
| Auto, one scan | Streaming | 6.992 s | 1.49 MiB |
| Auto, three repeated passes | Materialized | 7.595 s | 413.40 MiB |
| 64 MiB budget, three repeated passes | Streaming | 21.280 s | 1.49 MiB |

For this repeated-access workload, retaining data is about 2.8 times faster than rereading under the small budget. This does not accelerate the first XML parse. Auto defaults do not fill spare RAM or enlarge the input buffer without measured benefit. These are raw numeric scans/sums, not a general cache, editing, or text/style benchmark. Operation budgets reserve working components and constrain retained vector capacities, not whole-process RSS. The failed-estimate fallback is verified separately with deterministic heterogeneous fixtures.

## Cumulative layer probe

[layer-results.json](layer-results.json) profiles the original `345ae8fc50ea` reader on this input. Three measured runs after a warmup gave median 0.234 s for ZIP decompression/CRC alone, 4.264 s for decompression plus namespace-aware XML events, and 6.968 s for complete numeric streaming. These are cumulative diagnostic paths, not isolated function percentages. XML-only mode deliberately omits spreadsheet/value/resource validation and is not a production reader. The results direct future profiling toward XML and cell processing; they do not justify removing validation.

## Reproduction

Require Linux, `cc`, Rust 1.88.0 with Cargo available on PATH, and `python` with `openpyxl==3.1.5`. From the repository root:

```sh
python benchmarks/run.py --output benchmarks/results.local.json
python benchmarks/tune.py
python benchmarks/read_modes.py
python benchmarks/adaptive.py
python benchmarks/layer_profile.py
```

Generated inputs, native launcher, and Cargo targets are ignored. Input SHA-256 values record the actual measured files; regeneration may change ZIP timestamps while retaining the same cells. There are no fixed timing thresholds in correctness tests.

M2 typed boolean checkpoint and its measured numeric regression: [m2-boolean.md](m2-boolean.md).

M2 exact scalar fidelity, mixed inline text, and owned-payload budgets: [m2-scalars.md](m2-scalars.md).

Completed M3 sequential writer, public openpyxl readback and temporary-disk cost: [m3-writer.md](m3-writer.md).

M4 sparse/editor checkpoint: [report](m4-editor.md), [raw results](results/m4-editor.json), and `editor_checkpoint.py`. Existing-cell edit/save is compared directly with openpyxl general-mode load/edit/save; preservation, explicit sparse-model costs and reader regression are separate workloads.

Optional Python adapter: [same-call creation/edit comparison](python-adapter.md) includes interpreter/conversion costs, three alternating runs per engine and native RSS/temp/output measurements.

M5 A1 translation: [same-call Translator evidence and writer regression](m5-formula.md); full tokenizer/common-feature acceptance remains staged.

[Plain shared-string RAM/disk/Auto evidence](m2-shared-strings.md) includes repeated/high-cardinality text, full-value validation, disk/cache diagnostics and an explicit calamine speed gap.

Worksheet viewport metadata: [M5 worksheet views](m5-worksheet-views.md) records the selected rust_xlsxwriter port, public read/create/edit interoperability, model/overlay memory, temporary storage and unconfigured numeric regression. Workbook/chartsheet/custom views and the remaining M5 feature families stay open.

Printing metadata: [M5 printing](m5-printing.md) records the selected rust_xlsxwriter port, public margins/options/setup/page-property/break verification, full-source bounded reading, retained printer identity, footer/overlay memory, temporary storage and unconfigured numeric regression. Views/printing share the serial worksheet-feature harness; earlier evidence retains its recorded revision and samples.
