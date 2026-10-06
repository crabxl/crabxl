# Alpha.7 staged M4 acceptance

## Scope

This audit applies the agreed stage boundary in
[the release plan](../alpha-6-and-later-plan.md). It accepts supported canonical
loaded-model and structural operations, while keeping full M4 feature-graph
interactions assigned to M5/M6. Preservation is distinct from typed editing.
Publication and cross-platform checks are tracked separately below.

## Implemented behavior and evidence

| Stage requirement | Evidence |
| --- | --- |
| One canonical lazy bank for loaded and new sheets, stable handles, imported catalogs | ADRs 0055-0057; existing loaded ownership, aggregate failure and SST cleanup workflows |
| Shared model/overlay/catalog/working reservations, no whole-workbook rollback clone | ADRs 0056-0057 and 0072-0075; failed budget operations leave prior state usable |
| Scalar edits, insertion, append, row/column insert/delete, range copy/move and normal formula translation | ADR 0072; loaded sparse workflows and shared Python parity tests; coordinates, values, imported styles, logical extent and repeat saves verified |
| Physical-cell deletion and live aliases | `LoadedWorkbook::remove_cell`; loaded missing-cell exact-part preservation assertion; shared Python deletion/alias and graph-rejection workflow |
| Rename, reorder, visibility and active selection | ADRs 0058, 0060-0061 and 0069-0070; lazy identity, hidden selection, deferred indexes and repeat saves |
| Create, copy and remove worksheets | ADRs 0073-0075; mixed original/new/copied sheets, detached aliases, deleted-title reuse, retained template sources and final-visible-sheet recovery |
| Synchronized declarations, relationships and content types | Custom workbook paths, strict/prefixed namespaces, allocated-ID collisions, shared consumers and deleted-owned-part assertions in existing loaded workflows |
| Original source preservation and repeatable output | Scalar and catalog workflows retain unrelated XML/binaries; structural changes invalidate formula caches/chains and request recalculation; failure/retry and atomic path checks |
| Matching ordinary Python calls | Adapter ADRs 0017-0024; selected upstream assertions unchanged; 547 local compatibility cases pass after physical-cell deletion integration |

## Explicit dependencies

Affected row/column formatting, rich/structured or metadata-bearing cells, merges,
tables, validation, conditional formatting, comments, drawing/pivot graphs and
other unmodeled identity references reject structural changes until their M5/M6
codecs exist. Local defined-name owners and linked VBA identities also remain
tracked dependencies. Scalar preserving edits and unrelated sheet creation have
separate narrower guards. Unsupported operations fail before package mutation;
opaque preservation does not claim feature editing.

Typed loaded date/style replacement and complete style mutation remain M5 work.
Python file-like input/output remains explicitly unsupported. The reported
NYC_311_1M.xlsx is unavailable locally and has not been benchmarked.

## Performance evidence and limitations

[Structure](../../benchmarks/alpha7-loaded-structure.md),
[creation](../../benchmarks/alpha7-loaded-create.md),
[copy](../../benchmarks/alpha7-loaded-copy.md) and
[removal](../../benchmarks/alpha7-loaded-remove.md) record complete save/readback
workflows at 200,000 and 2,000,000 source cells, with release binary hashes,
checksums, wall/CPU time, peak RSS, managed accounting and sampled temporary-file
storage/cleanup. Measurements run serially without overlapping builds/tests.
Copies and detached removal intentionally transfer requested owned data; they
are not zero-cost metadata operations. Conservative virtual cell accounting is
still substantially larger than physical RSS and remains an A8 optimization item.

Read elapsed time still misses calamine on measured overlapping workloads;
measured Python RSS can be lower. No universal speed/RAM win or dataframe engine
integration is claimed. A8 retains these measured performance gaps.

## Verification and publication

Core checkpoint `39380a4cc9a5ceeff785d126749a13199efae88f` passes local workspace
tests, strict Clippy and loaded workflow tests on Rust 1.88. Adapter checkpoint
`2d9128b9326a036380383d811cc336495323e301` pins that exact core, builds a fresh
CPython 3.12 release wheel, passes Ruff format/check, strict Clippy and 547 tests.
Latest-core GitHub CI run 37398037891 is still completing platform checks at audit
creation. The alpha.7 version, registry publication, release artifacts and fresh
public-package installations are not yet verified; append exact evidence before
claiming release completion. Full M4 remains open under the agreed dependencies.
