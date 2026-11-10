# Status

Last update: 2026-11-10.

## Current release

- Nothing is published yet. The next release is `0.1.0`, the first one.
- The workspace version is `0.1.0`. The capabilities match openpyxl 3.1.5,
  and the performance and memory work is done (results in
  `internal/roadmap.md`).

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

Windows: 350 Rust tests pass. Formatting and Clippy pass for all workspace
targets. The Python acceptance (see the crabxl-python status) passes: 1,054
package tests, 1,548 corpus round-trip checks and the openpyxl 3.1.5 suite
with 2,115 passes and 329 tracked failures of openpyxl internals and
deliberate differences.

Windows Excel opens written and edited 1,000,000-row files with the right rows
and date formats. A full benchmark rerun and macOS Excel validation are
optional (see the roadmap).

## Deliberate boundaries

- CrabXL does not calculate formulas, chart caches or pivot results, execute VBA,
  or refresh external sources.
- Reference ranges, chart series and drawing anchors retain their explicit-edit
  policies during row/column edits. Notes move with their cells.
- Unknown affected markup and digitally signed edits return errors rather than
  dropping data. Untouched source parts retain their compressed payloads.
