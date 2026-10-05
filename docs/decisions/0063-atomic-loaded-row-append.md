# ADR 0063: Atomic append in source-backed models

## Decision

LoadedWorkbook appends scalar/formula rows through the canonical bank and the
existing original-package overlay. Materialize only the selected worksheet and
derive the position from actual decoded rows plus pending edits. Extend the
canonical logical extent for explicitly empty source rows; advertised dimensions
do not select the append cursor. Empty appends advance the in-memory cursor and
do not fabricate persistent empty cells.

Prevalidate every value, the complete overlay count/bytes and the model append
before committing. Bound the temporary row plan and value-vector scratch under
the joint retained allowance. Model and overlay each own their retained values;
account both copies, while cloning only the appended row. No worksheet or
workbook clone is used for rollback. Source SST/cache rebalancing may remain
after a failed operation, within its aggregate allowance.

The editor's row plan applies only to fresh append coordinates. Scalar/formula
upserts retain existing policies, including signature/calculation-chain checks,
non-finite behavior and explicit rejection of assigned dates or unresolved
phonetic font references. Supporting append does not complete date/style mutation
or M4 structural feature transformations. Unrelated opaque source assets remain
on the original seekable source and survive repeated saves.

## Verification

Extend the existing loaded-overlay workflow with undersized/oversized dimensions,
an explicit empty row, complete-row budget failure, late unsupported date
failure, successful retry, empty append, lazy unrelated sheets and repeated
save/reload retaining an opaque asset. Existing worksheet/bank tests exercise
owned append, bounds and failure behavior. Full workspace and strict lint checks
pass; the source-backed append benchmark verifies the added cells and count/sum
after saving, alongside kernel RSS and sampled working-file bytes.

## Performance evidence

One warmup and three serial release samples load both original numeric sheets,
append ten integers to the first, save and rescan the result. At 10,000 and
100,000 rows per sheet (200,000/two-million original cells), median complete
wall time is 0.5088 / 5.4612 seconds and peak RSS is 18,576 / 158,600 KiB.
Completed output and sampled temporary/output peaks are 607,233 / 5,887,748
bytes; no SST temporary files occur on these numeric inputs. Cleanup is checked.
This includes reload verification and is not comparable to a load-only timer or
proof of a speed improvement. Raw evidence is
[alpha7-loaded-append.json](../../benchmarks/results/alpha7-loaded-append.json).

The row cursor has no heap allocation. The underlying per-cell tree remains the
main editable-model allocation target; all-sheet-retained calamine comparisons
and packed sparse storage are separate measured performance work.
