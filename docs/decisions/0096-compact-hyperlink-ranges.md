# Compact hyperlink ranges

Decoded hyperlink ranges retain one payload per rectangle and a byte-accounted
geometry index. Point lookup borrows the covering declaration. The point-only
path retains logarithmic map access without scanning ordinary declarations.
Range lookup currently scans the range index; a measured interval-index upgrade
remains a performance dependency, not an assumed optimization.

Declaration adoption normalizes overlapping coverage in source order: a later
declaration replaces its covered coordinates. A rectangle difference yields at
most four disjoint pieces without enumerating cells. Point edits or clearing
inside a range retain the other pieces. Split planning accounts for final metadata
and the temporary owned payload before mutation; failure retains the old geometry.
Output and relationship resolution reuse the existing canonical hyperlink codecs.

Actual decoded coverage is distinct from a point object's independently mutable
serialized `reference`. Editing that reference alone must not populate new live
cells. `set_declaration` explicitly adopts coverage, while `set` updates an owner.
`covering_range` lets bindings project per-cell public references without copying
the rectangle's payload for every coordinate.

Physical cell removal now returns `Result<Option<Cell>>` because clearing a
covered declaration may require metadata allocation. The workbook and loaded
coordinators propagate that failure before removing a value. Existing test/example
call sites will be adapted in the concentrated pre-release acceptance pass;
library compilation passes on Rust 1.88. No new tests or benchmarks are added at
this implementation checkpoint.

Imported empty-cell display initialization, write-only live metadata, public
post-save identities, broader feature-aware shifts and consolidated Python/A11
acceptance remain open dependencies.
