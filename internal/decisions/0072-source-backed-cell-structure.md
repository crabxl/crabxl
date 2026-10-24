# ADR 0072: Source-backed cell structure

## Decision

Coordinate source-backed row/column insertion/deletion and cell-range
move/copy through the existing canonical Workbook bank. Materialize the selected
worksheet once, including pending scalar/formula overlays. Preflight the
original affected worksheet and package edit policies before mutation. Reuse the
existing atomic sparse core transformations and their aggregate working
allowances; do not clone a complete worksheet for rollback.

After the first successful structural edit, retain a bounded source-index to
stable-SheetId rewrite marker and release that sheet's coordinate overlays.
Subsequent scalar edits and row append operate on the same bank. Reserve the
larger of the old overlay and new marker charge until commit, so temporary
coexistence cannot borrow budget from overlays that are still resident. An edit
failure leaves prior cells/markers usable; lazy hydration or bounded source
cache preparation may remain after an unsuccessful operation.

Save borrows the canonical cells and imported style catalog, replaces sheetData
through the existing byte-bounded row encoder, and updates an existing declared
dimension from actual cell bounds. Preserve trailing empty-row extent separately.
Keep numeric temporal cells in their original number-format identities; temporal
values without a classified source style use ISO output. Styles are not cloned
or registered during this export. All formula caches are invalidated and the
existing calculation-chain discard/reject and recalculation policies apply.
Unknown unrelated parts stay on the seekable original and remain reusable across
repeated saves. Atomic path saves retain their adjacent-output failure policy.

Reject affected merges, column/row formatting, hyperlinks, tables, validation,
conditional formatting, drawings, worksheet extensions, cm/vm attributes,
structured/shared formula groups, rich inline runs, and rich/phonetic SSTs before
structural mutation. The SST guard is conservative across the referenced table,
including rich entries not selected by this sheet. These are visible M5/M6
dependencies, not removed features. Data-only structural edits and elapsed-duration style registration are unsupported.
Views/printing/properties outside sheetData are retained; insertion/deletion do
not invent automatic formula/name/view reference updates. An explicitly
translated range move uses the existing checked A1 translation engine.

The source/model transaction and borrowed catalog context are original CrabXL
implementation. The primary feature source inspected was umya-spreadsheet 3.1.0,
commit `aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b`,
`src/structs/{workbook,worksheet}.rs`, insert/remove coordinate methods and
Worksheet::move_range/copy_range. Its automatic cross-sheet reference
adjustments are not ported because the pinned openpyxl public contract does not
promise them. Cell serialization continues to reuse the existing licensed
rust_xlsxwriter-derived encoder; no additional imported engine/model is added.

## Verification

Extend the existing loaded overlay workflow with sparse shifts, moves/copies,
translated formulas, later scalar edits and append, style/date readback, trailing
empty rows, unchanged assets, repeat saves, bounds rejection, strict namespaces
and prefixed worksheets without a default namespace. Check affected graphs
reject without changing the model or output. Keep the number of test functions
unchanged. Run workspace tests, strict Clippy and Rust 1.88 loaded checks.

Measure release load/row-column insertion/save/full streaming readback separately
from read-only parser speed. Full-model memory scales with retained cells;
conservative managed/work ledgers are not physical RSS and can require an
explicitly larger budget than actual RSS. Loaded sheet creation/copy/removal,
macro/template policies and full deferred graph support remain separate M4 gates.
