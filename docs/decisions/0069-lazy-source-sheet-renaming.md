# ADR 0069: Lazy source-backed sheet renaming

## Decision

Keep source selectors, part names and relationship identities immutable. A
loaded bank's display title belongs to its stable SheetId, while the preserving
editor stores a bounded title patch keyed by the original catalog position.
Lazy materialization decodes the original source but constructs the model with
its current title. Cached cells and pending value overlays remain on the same
identity, without decoding cells during a rename.

Validate XML/name syntax, case-insensitive uniqueness, package signatures,
unsupported workbook alternatives and patch/joint allowances before changing
the model or catalog overlay. Account for the replacement title and metadata
node; reserve model growth through the bank's existing prospective allowance.
Repeated renames replace the charged title rather than accumulating history.
Save changes only the selected catalog attribute and preserves sheetId, r:id,
other attributes, unrelated parts and opaque assets. Pure title changes retain
formula caches/calculation chains and do not normalize the active view.

This coordinator is original CrabXL implementation. The pinned umya 3.1.0
`Worksheet::set_name` was inspected as the primary feature source; it also
updates worksheet-local defined names. The openpyxl 3.1.5 public probe retains
formula expressions and defined-name text on title changes, so that automatic
rewriting is not adopted. This is a title operation, not a typed name/metadata
graph editor. General creation, copying, removal, reorder and row/column
structural mutation remain separate M4 gates.

## Verification

Extend the existing loaded metadata workflow across transitional/strict,
custom workbook parts and opaque extensions. Cover lazy and already decoded
models, special-character/Unicode titles, stable handles, repeated saves,
duplicate/invalid names and atomic signed/alternative/patch-cap rejection.
Verify unchanged part bytes and source-backed values after rename. Run all
workspace tests, strict Clippy and Rust 1.88 loaded tests. Record the release
lazy rename/active/save/full streaming readback resource measurements separately
from parser speed and Python acceptance.
