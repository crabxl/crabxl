# Independent merged declarations and realized virtual cells

Native raw merge metadata edits are now distinct from high-level merge/unmerge.
Adding a declaration retains physical values and does not create virtual covered
cells. Removing or changing a declaration retains the already realized virtual
appearance pattern. Stable collection-local identities allow future bound views
to change declaration coordinates without using mutable coordinates as identity.

Ordinary unchanged merges retain the existing interval index. Only metadata
that diverges from realized cells creates detached compact appearance patterns;
no covered cell grid is materialized and no complete workbook is cloned.
High-level unmerge clears virtual coverage using at most four rectangle pieces
per affected pattern. Clipped pieces retain original edge geometry and appearance
IDs. Writer traversal, visible extent and structural guards include those patterns.
Metadata capacity participates in worksheet/workbook allowances. Source-backed
operations use the existing guarded preserving coordinator.

This checkpoint implements native ownership, raw mutation and serialization.
Python mutable bound sets, stable live coordinate views and native/Python bulk
replacement remain unfinished A11 work. The adapter still pins the earlier live-
hyperlink checkpoint; it does not expose these new mutation methods yet.

Rust 1.88 library compilation and formatting are the only checks at this stage.
No additional tests, performance claims or A11 acceptance are implied. The user
requested development to stop at this checkpoint; consolidated release acceptance
and remaining feature work are intentionally deferred.
