# Shared style components and bounded numeric-date interpretation

## Decision

The core owns optional font, color, pattern/gradient fill, nine-position border, alignment and protection components. Cell and run fonts are the same type; XLSX selects `font/name` or `rPr/rFont` in one parameterized codec. Missing properties and explicit false/zero remain distinct. The style catalog stores original component IDs, cell/base-format records, named-style metadata and indexed/recent palettes without expanding styles per cell.

The relationship-resolved style part is loaded lazily before selected rows or explicit catalog access. Encoded metadata shares the workbook metadata ceiling. Actual retained vector capacity and boxed payloads, including number-format classification, have `max_style_bytes` and per-table `max_style_records` limits. Advertised collection counts and custom format IDs never determine allocation. Numeric date classification is precomputed per imported cell format; quoted literals do not classify arbitrary numbers as dates.

Default `DateReadPolicy::Compatible` converts styled numbers and numeric formula caches to date/time/duration values in either epoch. Serial fractions are rounded separately to milliseconds, avoiding precision loss from scaling the absolute calendar serial. Unrepresentable baseline dates/durations become `#VALUE!`; `RetainSerial` is a distinct extension preserving finite source serials. Booleans, text and errors remain their own types under date formatting. Explicit materialization accepts the same read options as streaming.

## Evidence

Generated streaming/writer tests cover complete components, absent/false overrides, huge advertised counts, sparse custom IDs, component references, malformed properties, byte/count limits, zero-format dates, both epochs, leap-day ambiguity, clock rollover, durations, cached/missing formulas and materialized policies. Public constructor/conversion observations are recorded in `docs/research/style-date-public-probe.json`. Public style-creation assertions and mixed-date benchmarks are in `benchmarks/m2-styles-dates.md`.

## Remaining work and limits

This is a coherent catalog and numeric-date checkpoint, not complete M2. Theme resolution, full builtin catalog exposure, differential/table styles, ISO dates/durations, complete calendar construction precision and advanced formula metadata remain staged. Unmodeled style sections/XF extensions are reported as such; their payloads are preserved only through original-package preservation, not catalog export. New writer appearances still use the existing registration table; catalog normalization, safe imported mutation and loaded workbook bank integration remain required. Original-package date and new phonetic-font assignments are still guarded until safe editing integration.

Component and operation budgets are managed allocation limits, not hard RSS caps. Parser buffers and transient classification/gradient-validation work are additional bounded working allocations. Aggregate style/SST/materialized/overlay accounting and adaptive style allocation are unfinished. No benchmark against numeric dates establishes full style API parity.
