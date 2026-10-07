# ADR 0089: canonical point hyperlinks and owned-package relationships

Status: implemented native checkpoint; full A11 acceptance remains open.

## Ownership

Core owns `Hyperlink` optional target, location, display, tooltip and original
relationship identity. `Hyperlinks` owns sparse coordinate-ordered points with
cached node/text charging. Worksheet and workbook-bank mutation share this owner
and the existing allowance. Empty-cell assignment fills from target/location;
clearing a link retains its value. Physical removal clears both records.

XLSX owns namespace-aware decoding and relationship encoding. Explicit typed
reads scan the worksheet without buffering its XML or expanding declared ranges;
the normal scalar reader does not eagerly request this metadata. Relationship
metadata and returned links share the read allowance. Alternate valid Office
relationship prefixes resolve by namespace; duplicate expanded IDs reject.

Owned output borrows the canonical collection, emits links after merges and before
printing settings, and encodes external relationships under the writer allowance.
Active, paused and completed sheet owners account the actual encoded vector
capacities. Packaging uses final sheet order for relationship part names.
Relative external targets and URL fragments remain literal; XML escaping does
not alter decoded values. Source IDs are hints, not portable output identities.

## Failure behavior and remaining work

Metadata growth rejects before cell/link mutation. Affected row/column/move/copy
operations and merges over non-anchor links reject before losing declarations;
unrelated operations remain available. Internal-package target graph edits and
range declarations remain explicitly unsupported at this checkpoint. Full A11
must add applicable baseline range behavior, loaded preserving edits, copy
semantics and Python live public objects; this checkpoint does not close W13.

## Evidence

`canonical_hyperlinks_round_trip_and_reject_budget_growth_atomically` covers
external/relative targets, fragment/escaping, independent display/tooltip,
location-only links, source IDs, multiple sheet order, combined-budget rollback,
clearing/removal and affected structural rejection in one public round trip.
The generated `hyperlinks` example provides native create/save/scan phase timings,
checksums, managed peak temporary XML and Linux kernel peak RSS. Benchmark figures
are checkpoint probes, not a general read/write speed comparison.

Selected umya URL/tooltip/location separation and relationship ordering are
recorded in `third_party/ports.json`; storage, budgets and bounded codecs are
CrabXL implementations.
