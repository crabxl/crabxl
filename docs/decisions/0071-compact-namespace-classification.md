# ADR 0071: Compact semantic namespace classification

The checked namespace cache has six valid semantic states. Replace the prior
scope/optional-static-URI pair with a private exhaustive enum, projecting scope
and URI only when building the unchanged codec frame. Transitional and strict
spreadsheet namespaces remain distinct; relationship/content-type/drawing/other
states retain their prior URI behavior. Resolver ownership, delayed pops,
declaration snapshots, element nesting and all input/XML checks are unchanged.

This is an original representation refinement of the existing pinned quick-xml
ownership adaptation, not an alternate namespace grammar. On Rust 1.99 x86-64,
an equivalent standalone layout probe using the actual canonical Error type
measures classification 24 to 1 byte, successful Result storage 24 to 16 bytes,
and declaration snapshots 32 to 4 bytes. Layout is toolchain-specific and does
not establish whole-process RAM savings; ordinary documents have few snapshots.

Workspace tests, strict Clippy and Rust 1.88 streaming checks retain namespace,
prefix, strict URI, malformed input, resource, style and formula assertions.
Paired native release numeric/text/styled/SST measurements record small mixed
effects and remaining calamine speed gaps. No universal optimization claim or
new release gate follows from this representation change.
