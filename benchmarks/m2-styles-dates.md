# M2 imported styles and numeric dates

This checkpoint exposes complete optional appearance components and imported ID catalogs, and reads numeric dates, clock values, durations and date formula caches. M2 remains in progress.

Reproduce with `CARGO_TARGET_DIR=<target> python benchmarks/styled_checkpoint.py` after setting the Rust toolchain PATH. The generated workbook has ten columns of general floats, calendar dates, clock fractions, elapsed durations, styled booleans, inline text, errors, cached formulas and integers. Every cell is checked, including independently computed 2024 Gregorian dates. Native catalog counts and retained bytes are verified separately. The pinned public reference is openpyxl 3.1.5; calamine 0.36.1 participates only on overlapping numeric/date/cache values. Calamine exposes date/clock wrappers rather than identical Python/core types and materializes the entire range.

Linux results use one warmup and five rotating serial samples, release binaries, process wall/CPU and wait4 peak RSS. Builds and fixture generation are excluded; runtime baselines are included. The host is constrained to two CPU quota units and 8 GiB cgroup memory. No worksheet is materialized by the native reader and this fixture needs no shared-string or temporary storage. Managed retained style capacity is 2,829 bytes for both sizes, separate from process RSS and parser working buffers.

| Cells | Engine | Median seconds | CPU seconds | Peak RSS MiB |
| ---: | --- | ---: | ---: | ---: |
| 100,000 | calamine | 0.0380 | 0.0377 | 8.68 |
| 100,000 | crabxl | 0.0907 | 0.0906 | 1.94 |
| 100,000 | openpyxl | 0.5466 | 0.7038 | 33.48 |
| 1,000,000 | calamine | 0.3772 | 0.3760 | 73.18 |
| 1,000,000 | crabxl | 0.8990 | 0.8988 | 1.76 |
| 1,000,000 | openpyxl | 4.0521 | 4.1996 | 41.35 |

The required speed target against openpyxl is met on these workloads. Native speed remains below calamine, while native RSS is lower than both. This is not proof of full style API parity or whole-project performance acceptance. Full samples, generated input sizes/hashes and mode semantics are in [m2-styles-dates.json](results/m2-styles-dates.json).

Complete creation interoperability is independent of these numeric fixtures: `cargo run --release -p crabxl --example style_fixture -- <path>` followed by `python benchmarks/verify_style_fixture.py <path>` checks all font fields, path-gradient geometry/stops/color identities, nine border positions and flags, stacked rotation, simultaneous wrap/shrink, fractional indentation and protection through public openpyxl APIs. Results are [m2-style-creation-interop.json](results/m2-style-creation-interop.json). The generated source and creation sample are original fixtures, not copied upstream binary assets. Public constructor/date observations are reproducible with `benchmarks/probe_style_dates.py` and recorded under `docs/research`.

## Numeric hot-path regression

The prior core was built from exact published revision `512a958d761320b4a15c6194f04fb4c91dcebd8b` in an isolated checkout. Its binary was copied before rebuilding current source; different binary hashes are recorded. Both versions verify identical numeric count/checksum on existing generated unstyled fixtures, fixed default buffers and streaming mode. One warmup and five rotating serial samples exclude builds. Neither version loads a style catalog when no styles relationship exists.

| Cells | Previous seconds | Current seconds | Change |
| ---: | ---: | ---: | ---: |
| 100,000 | 0.0757 | 0.0749 | -1.1% |
| 1,000,000 | 0.7670 | 0.7725 | +0.7% |
| 10,000,000 | 7.5941 | 7.7908 | +2.6% |

The larger numeric case has a measurable positive slowdown that remains an M7 profiling task; no unmeasured cause is asserted. Complete samples, verified outputs and binary/input hashes are in [m2-styles-numeric-regression.json](results/m2-styles-numeric-regression.json). Error-message wording was clarified and additional precision tests were added after measurement; valid-path parsing/classification algorithms were unchanged. Benchmarks do not set ordinary test timing thresholds.

## Boundaries

The catalog retains absent versus false/zero, original IDs, named-style metadata and indexed/recent palettes. It is lazily prepared, charges actual capacities and payloads, bounds actual per-table records and never sizes allocations from advertised counts or sparse custom format IDs. Failed preparations retain no catalog. Streaming and materialized read options share default baseline-compatible numeric-date interpretation and the explicit `RetainSerial` extension. Public Gregorian rounding regression at year 9999 demonstrates why fractional-day milliseconds are computed separately.

Themes, ISO dates/durations, full calendar construction precision, advanced formula metadata, differential/table/extension typed editing and loaded bank integration remain staged. Unmodeled sections are listed; their XML is only preserved through the original-package path. New writer appearances still use the existing registration table; normalized imported catalog editing is unfinished. Catalog/SST/row/overlay budgets are not yet one aggregate hard ceiling, and no component policy guarantees a process RSS cap. Read-only samples do not compare rust_xlsxwriter because it has no overlapping reader API.
