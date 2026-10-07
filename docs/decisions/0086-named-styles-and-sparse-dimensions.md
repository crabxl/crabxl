# Named styles and sparse dimension ownership

Status: native W11 implementation checkpoint; consolidated A11 is not complete.

Canonical named styles reference shared base formats and component IDs. A
collision-checked name index resolves names without copying the catalog.
Registration reserves and accounts metadata capacities before insertion; failed
format registration rolls back logical name/base declarations while retained
capacities remain charged. Repeated name resolution deduplicates the derived
cell format. Normal references the actual default appearance.

Sparse row and column records retain explicit attributes independently of cells.
Shared StyleId replaces upstream per-dimension boxed styles. Worksheet edits
charge actual vector capacities; workbook editors validate local style links.
Column header reading stops before sheetData. Row metadata capture is opt-in on
the existing stream, leaving default scalar iteration without extra per-row
attribute decoding. Owned and loaded serialization merge physical rows and
metadata-only rows without expanding gaps. Original-package edits replace the
canonical columns and row attributes while preserving other supported metadata.
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

Selected tint/palette and row/column model layouts retain upstream MIT notices
and exact provenance in third_party/ports.json. Registry coordination and XLSX
integration are original CrabXL implementations. Python integration, merges,
outlines, hyperlinks and editable rich text remain necessary before A11 release.
Acceptance tests and profiling are concentrated after the coherent feature
integration, following the revised six-release plan.
