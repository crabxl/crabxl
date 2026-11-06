# Status

Last update: 2026-11-01.

## Current release

- Published Rust release: `0.1.0-alpha.11`.
- The combined A12 implementation pass (former A12-A15, W14-W26) is committed with
  a subsequent Rust regression and interoperability fix checkpoint. It is not a release.
- This pass changes Rust only. Python integration remains a separate task.

## Implemented Rust scope

- Existing cells, styles, tables, filters, sorting, rules, validation, notes,
  printing, protection, names and document properties remain integrated.
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
- Printer payloads and relationships participate in new and copied packages.
- Streamed snapshot and live-group hyperlinks merge with prebuilt feature
  footers, point declarations and relationship parts without a second worksheet
  or relationship envelope.
- Binary payloads can remain on disk. Loaded feature snapshots count toward the
  retained-data allowance. Metadata replacement validates existing cell indices.

Detailed native coverage is in `internal/features.json`; port origins and notices
are in `third_party/ports.json`.

## Validation and next stage

Windows validation passes: 335 Rust tests across 26 workspace test/doc-test
targets, including seven new complex-feature integration tests. Formatting and
Clippy for all workspace targets pass. Openpyxl 3.1.5 independently loads six
produced workbooks without warnings, checking charts, images, pivot records,
external links, templates and streamed hyperlinks.

Regression fixes cover drawing serialization, relationship capture and copying,
picture/frame dependencies, printer identities, metadata error context and
atomic rejected edits. The integration fixtures include independently generated
DrawingML and chartsheet printer dependencies.

Full release readiness, Excel desktop/cross-platform validation, performance/RAM
claims and Python acceptance remain pending. This checkpoint does not establish
complete baseline-module parity or M4/M5/M6 completion.

## Deliberate boundaries

- CrabXL does not calculate formulas, chart caches or pivot results, execute VBA,
  or refresh external sources.
- Reference ranges, chart series and drawing anchors retain their explicit-edit
  policies during row/column edits. Notes move with their cells.
- Unknown affected markup and digitally signed edits return errors rather than
  dropping data. Untouched source parts retain their compressed payloads.
