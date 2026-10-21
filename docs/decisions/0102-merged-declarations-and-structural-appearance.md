# Live merge declarations and sparse structural appearance

Raw merge membership now supports exact set insertion and atomic bulk replacement.
Unchanged declarations retain stable collection-local identities. Bulk planning
uses byte-accounted sorted coordinate/index scratch, rather than repeated whole-
collection searches. Declaration storage stays ordered by identity for logarithmic
live lookup. A failed preparation leaves membership and realized cells unchanged.

Structural edits move realized virtual cells independently of serialized merge
declarations, matching the observed openpyxl behavior. Rectangle intersections
and differences retain bounded pieces, including moved border masks and an
independent excluded anchor. Insert/delete/move/copy never expand covered grids.
Raw declarations keep their coordinates unless explicitly edited. Existing
physical cell and formula operations still perform their validation before
mutation. Formula translation/copy scratch shares the feature staging allowance.

Hyperlink owners follow structural cell changes while their serialized public
references remain independent. Compact coverage can split into rectangles;
per-cell public reference projection uses the original coverage offset. High-
level merge/unmerge drops non-anchor owners, retaining the anchor and unrelated
relationships. Loaded merges prepare and commit the hyperlink package rewrite
through the same preserving coordinator. Unknown or unimplemented source graphs
still reject before mutation under the existing M5/M6 ownership policy.

The native writer also provides a bounded closed-sheet output-identity visitor.
Its explicit live-group API remains independent of adapter close semantics:
Python uses separate groups per worksheet and stops updating a sheet's groups
after explicit close. No per-row Python alias cache is introduced.

This decision does not close full M4/M5/M6 or establish release acceptance by
itself. Consolidated A11 evidence must cover Rust/Python behavior, failure
atomicity, repeat saves, performance/resources and distribution checks.
