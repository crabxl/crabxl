# ADR 0023: Discarded formula semantics in cache-only reads

## Decision

Compatible data_only reads project the stored value without decoding formula headers, constructing expression strings or retaining shared-template state. XML character data/entities and formula structure still validate incrementally; duplicated formula elements, nested content, malformed cache values and CRC errors remain errors. Explicit ValidateGroups retains full formula/header/group validation even when projecting caches.

A separate seen-formula marker keeps duplicate detection and present blank text-cache behavior correct without allocating a dummy Formula. Existing selected value/date/style semantics remain unchanged. Individual XML-event limits remain enforced; the aggregate cell-expression payload limit does not constrain an expression the caller discards. Returned cache values remain subject to their own cell limits.

## Evidence

Public openpyxl 3.1.5 data-only probes return [2, None] for unknown formula types/invalid formula ranges and for a long discarded expression. Native tests verify these projections, a discarded 80,000-character entity-separated expression, strict rejection and malformed XML/cache/duplicate errors. Workspace tests, rustfmt and warning-free Clippy pass. Synthetic benchmark fixtures and public APIs were used without reference implementation inspection.

See benchmarks/m2-formula-cache.md. This is stored-value projection, not formula evaluation or typed graph completion.
