# ADR 0064: Bounded packed storage for canonical sparse cells

## Context

The user requires streaming and nonstreaming loading to target calamine speed,
and requires attempts to reduce nonstreaming cost below calamine. Dataframe
adoption also needs efficient native/binding conversion. The earlier reference
Range benchmark released each sheet separately, while CrabXL retained all
editable sheets; add an explicitly all-sheet-retained reference mode.

The canonical Worksheet stored a tree node entry/key for every Cell. This
retains ordering and random edits but incurs unnecessary allocation and tree
work during ordered full-model loading. Lowering measured RSS must not create
a separate read-only model or lose sparse/structural semantics.

## Decision

Store the same canonical Cells in ordered contiguous blocks, each containing at
most 128 cells. A BTreeMap indexes block first coordinates. Monotonic population
fills complete blocks and uses a cached last coordinate to reject missing
successor lookups cheaply. Existing cells remain sorted and uniquely addressed.

Random lookup resolves the preceding block then binary-searches its cells.
Interior insertion splits a full block before inserting; it shifts only bounded
block contents. Removing a block's first cell rekeys that block, and removing
the last cell refreshes the last-coordinate cache. Shrink materially underused
vectors after removal. Sparse row iteration starts at the preceding block and
visits only intersecting blocks. Retention/structural operations consume cells
and rebuild ordered blocks without cloning the whole model.

Public Cell references, styles, values, formulas, ordering, sparse addresses and
mutation facades retain the same contract. There is one cell store, not an
additional shadow index of payloads or a second dataframe-specific engine.
No unsafe code or upstream engine wrapper is introduced. This is original
canonical Rust model integration rather than an imported umya cell container.

Keep the existing conservative 256-byte node/payload charge per physical cell
and resource checks for this checkpoint. That ledger is not measured allocator
RSS and must not be presented as the actual packed size. Capacity/deletion
patterns, owned strings and styles affect real retention; do not infer a
universal memory reduction from compact numeric cells.

## Verification

Run existing workspace editing/serialization/resource tests and extend the
existing sparse-cell workflow across multiple blocks, reversed interior
insertion, boundary removal, complete removal/reuse and sparse row iteration,
checking results against an independently maintained ordered map. Run Rust 1.88
and strict Clippy checks. Compare identical prior/current numeric workers with
the same toolchain/lockfile, retaining both sheets in CrabXL and calamine. Verify
edit/create outputs, RSS, timing and working-file cleanup. Builds, tests and
profiling remain outside timed samples. Add text/style and binding evidence
before claiming broader performance results.

## Boundaries

Calamine Range remains a noneditable returned representation; CrabXL retains
editable models and source package ownership. Both retained-result costs are
visible rather than treating one-sheet Range RSS as the all-sheet cost. This
change does not itself solve XML parser throughput, all SST/string retention,
Python/Arrow conversion, or full M4/M6 acceptance.
