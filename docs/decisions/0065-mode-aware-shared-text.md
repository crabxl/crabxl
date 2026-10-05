# ADR 0065: Mode-aware immutable shared text ownership

## Context

Packed cells reduce canonical model overhead, but full loading still duplicates
shared-string payloads in the SST and in each retained cell. Repeated strings
are particularly expensive. Blanket reference-counted storage also increases
the allocation cost of high-cardinality streaming reads, whose rows are short
lived. Both streaming speed and complete retained-model time/RSS target calamine.

## Decision

Keep one canonical CellValue representation. CellText privately supports owned
boxed text and immutable Arc-backed text, with equality determined by content.
Full-model and loaded editable reads retain shared plain SST payloads; ordinary
row streams keep owned SST payloads and their existing conversion path. Only
one plain SST storage vector is populated. Rich values retain their existing
typed representation. Disk layout and disk allowances remain unchanged.

An ownership-mode change rebuilds the prepared SST. Adaptive repeated-access
sampling uses the materialization strategy, so its subsequent rebalance and
retained read reuse the same table. Sequential streaming uses owned storage.
Detached cloned values remain valid after source workbook destruction. Turning
shared text into an owned string explicitly copies the payload.

Managed accounting conservatively charges shared payloads and reference-count
headers for each alias. It is not an exact RSS estimate. Do not weaken resource
checks merely because physical string bytes are shared. Disk decoding can still
allocate a new shared payload when no decoded cache entry is retained; this
tradeoff requires separate disk-policy measurements.

## Verification

Extend existing value and lazy loaded-workbook workflows to cover content
equality, pointer sharing, detached lifetime, RAM/disk/Auto storage, repeated
saves and temporary-file cleanup. Retain the adaptive aggregate-budget tests.
Run workspace correctness, strict Clippy and the relevant Rust 1.88 tests.
Measure repeated/high-cardinality text models and row streams separately using
identical verified inputs, serial release samples and retained calamine ranges.

## Boundaries

This is original shared-model integration, not a second engine or an upstream
container wrapper. It does not itself complete M4, solve the parser throughput
gap, or implement Pandas/Polars integration. Numeric and styled workloads retain
their existing model and value semantics.
