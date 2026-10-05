# ADR 0060: Shared sheet visibility and source catalog edits

`SheetVisibility` belongs to the canonical worksheet model. New and copied sheets
start visible; explicitly hidden and very-hidden state survives model ownership
transfer, lazy source materialization and owned export. Catalog reads include
opaque chartsheets without claiming their typed content is implemented.

Original-package visibility edits share the bounded workbook metadata overlay
with active selection. Validate signatures, markup alternatives, source catalog
identity and prospective resource charges before changing either the loaded bank
or editor. Reserve active normalization together with the visibility overlay;
repeat changes reuse the charge. No worksheet cells need to load.

Explicit active selection requires a visible sheet. Intermediate all-hidden
states are permitted, but serialization rejects them before touching the output
sink. Hiding the current selection causes successful serialization to choose a
visible sheet at or after the current position, falling back to the first visible
sheet. The loaded coordinator synchronizes that selection after a successful
save. Borrowed owned-bank export normalizes its output without mutating the bank.

Workbook-only changes retain original worksheet values, formula caches, chains,
relationships and opaque assets. Names, original IDs, relationship IDs, namespace
context and unrelated catalog attributes stay intact. Source sheets are matched
only within the direct workbook `sheets` element, not extension subtrees.

Existing core identity, owned export and loaded metadata workflows cover copy
defaults, hidden/very-hidden states, strict/custom paths, lazy and materialized
models, repeated output, all-hidden failure and retry, normalization and atomic
signature/markup/resource rejection. No additional trivial test functions are
introduced. Python exposure and other sheet/package operations remain open; this
checkpoint does not close the M4 stage or publish Alpha 7.

Release measurements are in
[the visibility report](../../benchmarks/alpha7-sheet-visibility.md). Resource
charges remain constant across tested workbook sizes, with output values checked
in full. This is not a full-model or Python performance comparison.
