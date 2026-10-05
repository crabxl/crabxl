# ADR 0068: Binary boundaries within sparse cell blocks

## Context

Canonical packed cells index bounded ordered blocks. A row-range lookup begins
in the preceding block, then previously filtered every cell until the requested
row. Repeated row access scans unnecessary preceding entries, especially across
sparse empty rows. Bindings must use this common model rather than add an index.

## Decision

Keep the same block map and stored cells. Resolve the intersecting slice of each
candidate block using two partition points: the first key not less than the
inclusive start and the first key greater than the inclusive end. Iterate only
that borrowed slice. Missing/reversed bounds produce empty slices; no values,
rows or entire models are cloned and no extra index/allocation is added.

Preserve ordered unique cells, iterator cloning, exact endpoints and existing
row/column validation. Resource ledgers and physical storage do not change.
The original canonical implementation is independent of an upstream engine.

## Verification

Retain the existing independent ordered-map workflow covering multiple blocks,
backwards insertion, boundary deletion, empty rows and exact row traversal. Run
workspace tests, strict Clippy and Rust 1.88 worksheet tests. Compare identical
public-core release probes for dense/sparse rows, including gap rows, against
the prior core. Separate model generation/destruction from the reported inner
traversal timer, while recording full process wall/CPU/RSS and cleanup too.

## Boundaries

The native probe measures canonical model traversal, not XLSX parsing or Python
conversion and not calamine acceptance. Python must pin the published core and
verify end-to-end iteration, missing values and live edits/formula views before
claiming adapter speed gains.
