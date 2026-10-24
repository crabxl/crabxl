# Canonical catalog adoption checkpoint

One physical row has ten floats/date/clock/duration/boolean/text/error/formula-cache/integer values. Both native and public openpyxl 3.1.5 general-model calls retain declared catalogs, verify all values and change A1 to an existing source date format while keeping its scalar and source style ID. Native materializes the row, consumes the reader and transfers its catalog into the bounded registry without cloning; all source component IDs remain stable. No save or complete loaded-bank editing claim is made.

One warmup and five rotating serial cold-process samples; no builds/checks or temporary storage during measurement.

| Declared formats | Current seconds / RSS KiB | openpyxl seconds / RSS KiB | Native managed registry bytes |
| --- | --- | --- | --- |
| 1,001 | 0.002252 / 2,120 | 0.180456 / 35,432 | 114,204 |
| 50,001 | 0.042801 / 8,836 | 0.454393 / 90,484 | 4,728,596 |

Required public-reference speed and desired public-reference RSS hold on these catalog-heavy model-call workloads. Extra declared formats are deliberately unused by physical cells, exposing catalog/index costs; this is not an ordinary many-row performance claim. No previous canonical adoption API existed, so a previous equivalent import-engine timing is not fabricated. No calamine/rust_xlsxwriter comparison is claimed.

Reproduce with style_import_checkpoint.py and style_catalog_import; raw results: results/m2-style-import.json. Sparse/duplicate/override identities and malformed/budget failures are separate deterministic tests. Allocator/dependency overhead and caller-held source objects remain additional to managed allowances. No milestone completion is claimed.
