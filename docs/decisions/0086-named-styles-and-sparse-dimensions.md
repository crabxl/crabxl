# Named styles and sparse dimension ownership

Status: native W11 implementation checkpoint; consolidated A11 is not complete.

Canonical named styles reference shared base formats and component IDs. A
collision-checked name index resolves names without copying the catalog.
Registration reserves and accounts metadata capacities before insertion; failed
format registration rolls back logical name/base declarations while retained
capacities remain charged. Repeated name resolution deduplicates the derived
cell format. Normal references the actual default appearance.
Named appearance updates retain existing cell-format identities while future
applications resolve the new appearance. Renames and declaration metadata edits
update the same collision-checked index, reserving before logical mutation.

Sparse row and column records retain explicit attributes independently of cells.
Shared StyleId replaces upstream per-dimension boxed styles. Worksheet edits
charge actual vector capacities; workbook editors validate local style links.
Column header reading stops before sheetData. Row metadata capture is opt-in on
the existing stream, leaving default scalar iteration without extra per-row
attribute decoding. Owned and loaded serialization merge physical rows and
metadata-only rows without expanding gaps. Original-package edits replace the
canonical columns and row attributes while preserving other supported metadata.
Row grouping reserves once and merges metadata in place, preserving sparse
existing heights and styles; column grouping retains the first declaration's
appearance. Sequential writers retain bounded metadata per live spool, flush
column declarations before the first row, and emit metadata-only trailing rows
before closing sheetData. Late changes to already spooled rows/columns reject.
Unknown affected attributes/graphs still reject before mutation. Extended row
descent serialization remains explicitly unsupported until extension namespaces
are integrated.

Theme color resolution retains raw color identity and tint. The selected pinned
umya algorithm uses 255-step HLS quantization; zero tint retains exact channels.
This does not claim complete Excel rendering equivalence. Unknown theme slots
and automatic colors resolve to None; portable system-color fallback is used
when supplied. Exact theme bytes can be replaced at an existing relationship
target without rewriting unrelated drawing sections. Signed-theme mutation and
creation of a missing source theme relationship remain explicit graph gaps.
Clearing a loaded theme restores the standard theme at its existing target.
Owned-model writers adopt exact canonical style catalogs without injecting
automatic temporal presets; resolved temporal IDs stay workbook-local.

Selected tint/palette and row/column model layouts retain upstream MIT notices
and exact provenance in third_party/ports.json. Registry coordination and XLSX
integration are original CrabXL implementations. Python integration, merges,
hyperlinks and editable rich text remain necessary before A11 release. Native
outlines and the initial Python dimensions/named-style adapters have passed
focused owned/loaded/sequential interoperability checks, but complete binding
compatibility and consolidated release acceptance remain pending.
Acceptance tests and profiling are concentrated after the coherent feature
integration, following the revised six-release plan.

## Focused sequential-dimension checkpoint

A CPython 3.12 development wheel and openpyxl 3.1.5 generated 100,000
metadata-only grouped rows, then every result was reopened with openpyxl and
checked for row count, final-row outline level and visibility. Two alternating
runs per engine, without overlapping builds, found repeated linear searches in
CrabXL's pending-row emission. Sorted partition lookups removed that repeated
scan: CrabXL save time changed from 3.38–3.69 seconds to 0.0368–0.0369 seconds.
The corresponding openpyxl runs took 0.515–0.551 seconds. Group creation took
0.0025–0.0041 seconds in CrabXL and 0.440–0.464 seconds in openpyxl.

Peak process RSS, captured before reference reopening, was 23,048–25,272 KiB
for CrabXL and 89,332–89,660 KiB for openpyxl; output sizes were approximately
265 KB and 260 KB respectively. Each engine used its default compression and
resource settings. These are limited development measurements with two samples,
not full A11 performance acceptance or a general spreadsheet speed claim.
Scalar, styled-cell and mixed structural workloads remain release gates.
