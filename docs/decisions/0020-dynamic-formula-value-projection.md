# ADR 0020: Baseline dynamic formula value projection

## Decision

The pinned public reader ignores opaque cell cm/vm indices when returning scalar values, array formula objects and cache-only results. Generated observations include zero, negative, huge and nonnumeric index spellings; these do not change visible values. Core array expression/range/cache models already represent the baseline dynamic formula object. The XLSX reader therefore projects visible values under CellMetadataReadPolicy::Compatible instead of rejecting an annotated cell. Reject is an explicit extension for callers who require unresolved graph references to fail. Successfully returned annotated cells are counted by Rows::projected_metadata_cells. Excluded cells do not trigger rejection or count as returned projections. No per-cell metadata allocation or larger Cell layout is introduced.

This policy does not parse, validate or retain cm/vm graph identities in an owned scalar snapshot. It follows the baseline visible-value semantics rather than claiming typed metadata support. Original-package preservation remains independent; untouched references and opaque metadata parts are retained by the editor.

Direct physical-cell replacement drops the target's old cm/vm references because they describe the old value, matching public replacement behavior. Other cells keep their source references and metadata parts remain unchanged: removing a consumer does not renumber graph records. Array replacements continue to use canonical formula codecs; shared-group replacements retain their guard until group normalization. No other coordinates, spill extents or graph entries are changed, and no formula calculation is added.

## Evidence and remaining scope

Public/native expression/range/cache and edit readback cover five index spellings. Deterministic tests cover projection/rejection, excluded cells, contextual failures, repeated saves, unaffected references and exact opaque metadata parts. Representative dynamic read and plain-formula regression samples are in benchmarks/m2-dynamic-formulas.md. Source observations use public APIs and generated OOXML only.

Full typed metadata/dynamic extension graphs, resolved spill dependency behavior, metadata creation and structural reference editing remain M4/M6 work. These are not silently marked implemented by visible projection. Aggregate budgets and complete loaded style/catalog ownership remain open; M2 as a whole is not complete.
