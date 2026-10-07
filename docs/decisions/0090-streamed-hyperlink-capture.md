# ADR 0090: opt-in declaration capture and lazy canonical source access

Status: implemented read checkpoint; preserving hyperlink mutation remains open.

## Shared capture and source ownership

One namespace-aware `Capture` decodes hyperlink declarations. The explicit metadata
scanner and opt-in row-stream tail use it; there is no second point decoder.
`Rows::capture_hyperlinks` leaves the ordinary scalar path disabled. Fully consuming
rows captures compact point metadata; `take_hyperlinks` transfers it after EOF.
`WorkbookReader::resolve_hyperlinks` resolves IDs after the stream borrow ends,
without scanning worksheet XML again. Location-only declarations need no relationship
file. Source IDs, external/relative targets, tooltip and display remain independent.

`LoadedWorkbook::hyperlinks` borrows the collection in the same canonical bank.
A typed request before cell materialization enables capture in that one cell scan.
A request after materialization performs one explicit metadata scan; later requests
borrow the cached owner. Ordinary cell materialization keeps capture disabled so
unsupported feature declarations do not prevent unrelated scalar reads. SourceSheet
stores only request/loaded policy flags, never a duplicate collection. Adopting
metadata retains the model's previous dirty state; a getter is not a user edit.

## Budgets and failures

Merge declarations and hyperlink nodes share captured metadata accounting with the
aggregate row pool, formulas and SST. Capture reserves prospective metadata before
inserting nodes, allowing Auto SST spill or cache reduction first. Target resolution
owns the input collection, checks coordinate scratch and parsed relationship/text
charges, and rejects target growth before allocating another target copy. Failures
do not publish partly resolved metadata. Relationship parsing now charges decoded
records as well as input XML; configured metadata budgets cover both.

Loaded temporary cells, raw declarations, merge geometry, relationship resolution
and source/model reservations participate in the same retained allowance. A failed
capture/resolution drops the incoming model; late adoption commits only a validated
collection. Metadata access itself does not queue a source part rewrite.

## Verification and remaining work

The existing public point round trip additionally consumes rows with capture,
resolves their IDs, compares the explicit scan, and exercises both source access
orders. Repeat getters retain two declarations; models stay clean and source saves
retain targets. Rust 1.88 workspace tests and Rust 1.99 strict Clippy pass.

[Native measurements](../../benchmarks/alpha11-hyperlink-capture.md) distinguish
metadata scanning, combined row/capture/resolution and explicit loaded models.
There is no claim of a general speedup or whole-process memory ceiling.

Range declarations, preserving edits/relationship graph updates, copied-source
ownership and Python live public objects remain A11 gates. Internal package target
graph editing remains a later complex graph dependency. This checkpoint does not
release A11 or close W13.
