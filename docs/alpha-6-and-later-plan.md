# Alpha.6 and later release plan

## Release order

This plan follows alpha.5 and records the agreed staged M4 acceptance boundary.
All releases below are planned, not completed or published.

The active delivery goal includes completing and publishing A6 through A8,
then implementing all M6 acceptance groups and publishing multiple usable
Alpha releases. M6 planning alone does not satisfy this goal. M7 remains the
subsequent full compatibility and release-quality milestone.

| Release | Scope | Acceptance boundary |
| --- | --- | --- |
| A6: `0.1.0-alpha.6` | M4 loaded-model integration, existing-file edits, worksheet and row/column operations | Complete the M4 stage defined below; retain explicit M5/M6 dependency tracking |
| A7: `0.1.0-alpha.7` | Remaining read, write, edit, and Python binding performance problems | Profile and fix the measured backlog, with equivalent-workload evidence and correctness/resource checks |
| A8: `0.1.0-alpha.8` | M5 common features | Full assigned style, merge, table, validation, conditional-formatting, comment, printing, and formula-tool acceptance |
| Subsequent alphas | M6 advanced graphs | Release usable, independently verified feature groups; choose each next alpha number manually |
| M7 completion release | Full compatibility, platform support, performance, and release quality | Complete the full baseline acceptance matrix, including deferred M4 graph interactions |

Rust and Python ship corresponding functionality. Rust uses the
`0.1.0-alpha.N` tag format; Python packages use `0.1.0aN`. The repositories keep
independent release histories, so matching numbers require checking both before
preparation. Do not change versions or submit release requests just to record a
plan. Publish only after a usable checkpoint meets its gate.

## A6: M4 staged acceptance

### Current gaps

The canonical owned workbook bank already supports sparse scalar/formula models,
stable sheet identities, aggregate model allowances, and owned sheet operations.
The existing-package editor already supports bounded cell overlays, repeat saves,
atomic path output, and unrelated-part preservation. These are foundations, not
complete loaded-model integration.

At the alpha.5 checkpoint, loaded Python worksheets materialize as separate
standalone models. Loaded models, source catalogs, SST/cache, and editor overlays
have separate component allowances. Loaded append, row/column structural edits,
sheet creation/copy/removal/rename/reorder, and several workbook mutations remain
explicitly unsupported. General non-consuming styled model export and loaded
feature-graph transformations remain open.

### Implementation order

1. Integrate lazy loaded worksheets into the canonical workbook bank with stable
   handles and shared source catalog identities. Support mixed original/new sheets
   without a second binding-owned spreadsheet engine.
2. Define and enforce aggregate managed accounting for retained catalogs, models,
   overlays, caches, and operation working space. Avoid whole-workbook copies as
   rollback or serialization strategies. Document additional Python/dependency/
   allocator/OS costs and caller-retained detached objects separately.
3. Centralize package relationships, content types, part allocation, source IDs,
   dirty state, and dependency ownership. Preserve unrelated unknown XML and
   binary assets through the original source rather than resident byte copies.
4. Implement loaded sheet create/copy/remove/rename/reorder/visibility/active
   selection for supported content, keeping identities and declarations consistent.
5. Implement loaded append and sparse row/column insert/delete/copy/move. Reuse
   canonical coordinate, formula, value, and style logic. Follow the pinned public
   reference's actual structural behavior rather than inventing automatic updates
   to all formula or feature references.
6. Expose the verified operations through matching Python calls, retaining live
   cell/sheet alias behavior, explicit resource controls, and original-part saves.
7. Complete stage acceptance and publish Rust crates, then Python wheels/sdist
   pinned to the published core revision. Verify fresh registry installations.

### Stage boundary and deferred dependencies

The user explicitly selected staged M4 acceptance. A6 completes loaded ownership,
package/sheet mutations, and structural operations for implemented content. An
operation affecting an unimplemented M5/M6 graph must return an explicit
unsupported error before mutation or target replacement. Unaffected opaque parts
must continue to survive supported edits.

A6 does not complete all M4 interactions with future styles, merges, tables,
validation, conditional formatting, comments, drawing anchors, charts, pivots,
external links, or complex cell metadata. Track these dependencies in the
existing behavioral inventory and completion plan; verify them when the owning
M5/M6 family is implemented. Report **M4 stage accepted for A6**, keeping full M4
acceptance open until the dependency cases pass. Preservation alone does not
verify typed reading, creation, or editing.

### Required evidence

- Existing and mixed original/new worksheet workflows, sparse boundaries,
  repeated saves, and reload values/styles/formulas/worksheet ordering.
- XLSX/XLSM/XLTX/XLTM preservation and applicable macro/template policies;
  remaining unsupported policy mutations identified explicitly.
- Retained handles across rename/reorder/copy/removal, source lifetime/close,
  failed budget mutations, failed-save retry, and temporary-file cleanup.
- Namespace variants, custom part names, shared consumers, and unchanged opaque
  assets. Affected unsupported graphs reject atomically with useful context.
- Meaningful public-reference interoperability and failure assertions, extending
  existing tests where practical. Preserve the selected upstream test bodies.
- Relevant native and Python measurements, including wall/CPU time, peak RSS,
  temporary bytes/cleanup, and actual output checks. A7 is not a reason to defer
  resource accounting or introduce unmeasured regressions in A6.

## A7: Performance backlog

Start from the published A6 revision. Record specific measured problems and their
locations before choosing optimizations. A7 covers the backlog across native
processing and Python calls; it is not limited to compression or one fixture.

Investigate the calamine read-speed gap, XML/value parsing and allocation, SST
lookup/cache/spill behavior, style/formula preparation, sparse mutation, repeated
saving, compressor throughput, and Python/native conversion and call frequency.
Add the missing direct rust_xlsxwriter comparison. Compare ordinary, streaming,
and editable modes separately using equivalent supported behavior.

Use pinned reference versions, warmups, alternating repeated release runs, and
numeric/text/styled/sparse/multisheet workloads at representative sizes. Record
verified outputs, wall/CPU time, peak RSS, temporary storage, output sizes, and
cleanup. Keep resource and compression settings visible. Tune Auto strategies or
concurrency only when measured gains justify their memory/I/O cost.

Every fixed item needs before/after evidence. Record unresolved items and
tradeoffs explicitly; publication must not imply universal superiority or that
no further optimization is possible. Extend existing high-value correctness
checks rather than adding timing thresholds or redundant test permutations.

## A8: M5 common features

Complete the existing M5.1–M5.8 acceptance definitions: styles and rich text;
dimensions, merges, groups and views; names, hyperlinks and properties; tables,
filters and sorting; validation and conditional formatting; comments; printing
and protection; formula tokenizer and translation tools. Formula calculation is
outside the baseline.

Each family requires shared models, applicable readers/writers/editors, Python
compatibility, bounded resources, and verified save/reopen behavior. Typed core
models alone do not complete Python APIs. Resolve the associated deferred M4
structural cases as each family becomes available.

## M6: Multiple usable alpha releases

Use the existing M6.1–M6.4 groups as planning boundaries: images/drawings and
anchors; charts/chartsheets; pivots/caches/records; external links, macro policies,
extensions, dynamic-array/rich-value and cm/vm metadata graphs. Split further when
necessary to deliver coherent usable capabilities; do not fix the alpha count in
advance or mark a whole family complete from a narrow checkpoint.

Every release must pair the relevant read/create/edit behavior with relationship
integrity, preservation limits, repeated saves, bounded binary/record handling,
source ownership, and corresponding Python support. Complete its deferred M4
interaction cases and retain any remaining cases visibly.

## M7: Final acceptance

M7 verification accompanies development and closes after the full baseline
matrix passes. Complete cross-platform/MSRV and Python ABI acceptance, broad
interoperability, corruption/resource/failure handling, final profiling, docs/API
examples, licenses/provenance, packaging, and fresh public-install verification.

Retain Rust 1.88 MSRV unless explicitly changed; Python builds use the latest
validated stable Rust toolchain. Preserve the agreed five wheel platforms and
CPython 3.11–3.15 coverage, replacing the Python 3.15 RC exception with stable
validation when available. Full milestone closure requires the outstanding M4
and M5/M6 graph cases, not an inventory count or preservation-only claim.
