# Sparse merged geometry and canonical source editing

Status: usable W12 native/Python checkpoint; consolidated A11 is not complete.

`MergedRanges` owns finite rectangles and sixteen shared style identities per
rectangle. An augmented AVL interval index resolves containment and the latest
covering non-anchor appearance without expanding the covered area. Both actual
vector capacities are charged. Insertions and point queries use the interval
index; exact removal rebuilds it in existing storage. Bounded in-order iterators
use a fixed stack and borrow styled ranges for row emission. This index is CrabXL
code; only the original compact range collection layout is adapted from pinned
umya-spreadsheet, with provenance in `third_party/ports.json`.

The workbook coordinator combines anchor and bottom-right borders, interns edge
and protection appearances, reserves geometry, and removes covered physical
values in place. The anchor retains its value and appearance. Non-anchor values
are read-only. Protection comparison uses effective defaults so explicitly
setting locked/visible defaults does not manufacture styles over an entire
rectangle. Logical append position remains distinct from visible merge extent.

Owned and source-backed output borrow physical cells and virtual appearances.
Default-style merges use the ordinary sparse physical-row encoder; styled merges
query indexed intervals and skip unstyled gaps. No dense cell grid or whole-row
clone is created. Explicit styled covered coordinates can still increase XML and
output time in proportion to the represented styles. A full-grid default merge
retains one physical anchor and produces a small worksheet body.

Row streams capture raw merge declarations only on opt-in, sharing their existing
XML scan through EOF. Raw capacity is included in the retained allowance. Loaded
models normalize after decoding under joint source/model/catalog/SST allowances;
failed normalization does not commit the incoming sheet. Plain loading leaves
original parts untouched. Canonical model rewrites or copies export newly
resolved styles when needed, keeping source identities and repeat saves valid.
Unknown affected merge attributes/namespaces reject before mutation.

Python exposes high-level `merge_cells`/`unmerge_cells`, lazy `MergedCell` views,
finite bounds and live merge membership. Only held aliases are detached after a
successful merge/unmerge; the rectangle is not expanded. Source overlays retain
bounded pending-value access even when another model cannot fit the joint cap.
Deferred non-anchor overlays reject on preserving save instead of losing values.
Raw live membership/coordinate mutation remains explicitly unsupported, rather
than silently modifying detached snapshots.

Affected row/column insertion, deletion or range movement remains an explicit
structural dependency. Appending over covered coordinates rejects before mutation;
appending beside a range retains the logical append position. Source files missing
style relationships, rich/unknown affected graphs and data-only structural edits
retain their existing explicit limitations. These remain tracked in consolidated
A11/A15 acceptance; this checkpoint does not close M4/M5 or publish A11.

Existing native workbook/writer/loaded regressions cover full-grid sparse geometry,
indexed queries, reverse insertion/removal/overlap precedence, readonly values,
source guards and repeated output. One public Python scenario runs against both
engines for ordinary/loaded merging, border/protection propagation, aliases,
source preservation and repeat saves. The retained suite includes aggregate
memory failure and pending-overlay recovery checks. See the narrow
[merge measurements](../../benchmarks/alpha11-merged-geometry.md).
