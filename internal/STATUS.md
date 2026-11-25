# Status

Last update: 2026-11-25.

## Current release

- Releases and their changes are in `CHANGELOG.md`. Version 0.3.0 (unreleased)
  redesigns the public API: `Workbook`, `read::Reader` and `write::Writer`.
- The Python package offers the native API under `crabxl` and the openpyxl
  3.1.5 interface under `crabxl.openpyxl`.

## Implemented Rust scope

- Cells, styles (including `quotePrefix` and `pivotButton` components),
  tables, filters, sorting, rules, validation, notes, printing, protection,
  names and document properties.
- Sheet properties (`sheetPr` with tab color and outline settings) and sheet
  format properties (`sheetFormatPr`) on every read, write and edit path.
  `PrintSettings::properties = None` keeps the source `sheetPr`.
- Scenarios are read, written, edited and kept on structural edits.
- Images, one/two-cell and absolute anchors, shapes, connectors, grouped objects,
  drawing dependencies and picture/frame hyperlinks.
- All 16 baseline chart-family variants, series and editable chart properties,
  including combinations, axes, layouts, labels, formatting and extensions.
- Chartsheet creation, properties, drawings, related assets, copy and removal.
- Pivot definitions, shared caches, owned dependencies and bounded record
  reading/writing. Definitions and layouts are editable without calculating results.
- Ordered external-link caches; VBA Preserve/Remove/Replace; XLSX, XLSM, XLTX and
  XLTM formats; metadata, connections, rich-value and cm/vm graphs.
- Sheet copying rebuilds owned graphs and assigns unique table/pivot names.
  Graph removal keeps parts with surviving consumers. Notes follow cell edits.
  Resolved shared formulas can be structurally edited using expanded expressions.
- Data-only model edits and saves project source formulas to cached scalar values.
- The interleaved (write-only) writer accepts views and printing before the
  first row (`set_interleaved_header`) and features, printing and merged cells
  after the last row (`set_interleaved_tail`), in schema order.
- New sheets in loaded workbooks keep their views, sheet properties, format and
  printing on save.
- Binary payloads can remain on disk. Loaded feature snapshots count toward the
  retained-data allowance. Metadata replacement validates existing cell indices.

Port origins and notices are in `third_party/ports.json`.

## Validation

Linux, 2026-11-25: 408 Rust tests pass, including doctests and the public
API tests in `crates/crabxl/tests/api.rs`. Formatting and Clippy pass for all
workspace targets. The Python validation (see the crabxl-python status)
passes: 1,100 package tests and the openpyxl 3.1.5 suite with 2,114 passes
and 330 tracked failures of openpyxl internals and deliberate differences,
unchanged from 0.2.2. Files from the new API open in openpyxl with the
expected values, formulas, styles and frozen panes.

Windows Excel opened written and edited 1,000,000-row files from 0.2.x. The
0.3.0 output has not been opened in Excel yet (see the roadmap). The
benchmark suites are in crabxl-benchmarks.

## Deliberate boundaries

- CrabXL calculates formulas only through the optional `calc` feature, and not
  array or data-table formulas. It does not calculate chart caches or pivot
  results, execute VBA, or refresh external sources.
- Row and column edits update formulas, names, tables, rules, merges,
  dimensions, chart series, anchors and internal hyperlinks. Pivot tables,
  scenarios, array and data-table formulas and 3-D references cannot follow;
  such edits fail before anything changes.
- A save that would remove content CrabXL cannot keep fails unless
  `DataLossPolicy::Allow` is set. Untouched source parts retain their
  compressed payloads.
