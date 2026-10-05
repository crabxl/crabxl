# ADR 0058: Lazy active-sheet selection in original packages

## Decision

`WorkbookEditor::set_active_sheet` changes the first original workbook view's
active tab without decoding worksheet cells. `LoadedWorkbook::set_active_sheet`
uses the same codec with a stable core ID and synchronizes the canonical bank
after pure policy/metadata validation and prospective joint resource reservation.
The active overlay has one conservative fixed charge, reused by later changes;
it participates in existing patch limits and loaded aggregate accounting.

Selection rejects unknown IDs, hidden/very-hidden targets, signed packages,
unsupported markup-compatibility alternatives and exceeded budgets before
changing either active state. The metadata scan is bounded by canonical XML and
metadata policies and reads only the original workbook part. It is not a full
sheet scan or a second resident workbook graph.

Saving rewrites only workbook metadata for an active-only change. Existing view
attributes and additional views remain intact; missing or empty bookViews get
one namespaced workbookView in the correct position before sheets. Strict and
transitional namespace context and normalized discovered workbook paths are
retained. Active-only saves keep worksheet bytes, formula caches, calculation
chains and their relationships unchanged. Value edits still use the existing
cache invalidation policy independently. Repeat saves retain the overlay and
source; standalone editor `clear_edits` also clears active selection.

Original chartsheets can be selected while their content remains opaque. That
does not implement typed chartsheet models. Sheet rename/order/create/copy/remove,
visibility mutation, full workbook-view models and Python exposure remain
subsequent M4/M5 checkpoints; this is not A6 acceptance or full M4 closure.

## Verification

The loaded suite adds one meaningful workflow covering existing/missing/empty
views, strict namespaces, custom workbook paths, no eager cell materialization,
stable active IDs, repeated saves and returning to the original selection.
Actual formula-cache and calculation-chain fixtures retain every non-workbook
part exactly. Hidden, signed, alternative and patch-cap failures leave the
original active ID, overlays and retained charge unchanged. Existing chart
preservation tests also select the opaque chart before an unrelated value edit.

Workspace tests, Clippy and warning-free documentation pass. Rust 1.88 exercises
the loaded/editor suites and the native-zlib loaded path. Representative release
time/RSS/output/storage evidence is in
[active-selection measurements](../../benchmarks/alpha6-active-selection.md).
