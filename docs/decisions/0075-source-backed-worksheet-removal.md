# ADR 0075: Source-backed worksheet removal and ownership guards

Status: implemented for the staged supported subset; Python integration and final
A7 acceptance are pending.

Removal transfers the requested canonical model to the caller and permanently
invalidates its registered bank identity. Other models are neither cloned nor
replaced. The detached model retains its own edit limits; its caller-owned data
is no longer part of the loaded bank's managed allowance.

A bounded live membership transaction retires the source worksheet part, its
empty owned relationship part, catalog declaration, workbook relationship and
content-type override together. Pending scalar/model/view/printing/name/state
patches for that original identity are retired with their exact ledger charges.
Newly created worksheets are simply retired from the pending part registry.
Former source declarations and the immutable archive remain available as
property templates for surviving copies. Reusing a deleted display title must
not reconnect old handles to the new identity.

The same checked incoming-edge scanner now serves calculation-chain and
worksheet ownership. Removal accepts only the selected declared workbook
relationship as an incoming owner; unknown/shared consumers and outgoing graph
edges reject before materialization. Outgoing external edges and linked VBA projects also reject; VBA may name
sheets without an OPC edge to a worksheet part. Macro/template main content
types and unrelated binary assets remain preserved by supported scalar edits.
No relationship DOM or worksheet XML is retained.

Some owners reference sheet names/IDs outside worksheet relationships. Until M6
implements that analysis, source structural edits/copies/removal reject workbook
pivot caches, custom workbook views and extension containers conservatively.
Markup-compatibility alternatives also reject. This explicitly retains the M6
dependency instead of leaving stale caches/owners; scalar preserving overlays and
unaffected opaque package parts remain separate supported paths. Local defined-name
ownership stays an M5 dependency. Copy context checks happen before hydration.

All sheets can be removed in memory, matching the public reference. Save rejects
before writing a workbook with no visible sheets; creating a visible sheet makes
it recoverable. Raw active display indexes retain the existing deferred policy.

This is original CrabXL coordination, with the pinned umya-spreadsheet 3.1.0
workbook sheet-collection/remove behavior at
`aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b` as the primary inspected reference.
No upstream implementation was imported for this package transaction.

Existing loaded workflows verify original/new removal, detached values, title
reuse, surviving copies, empty relationship cleanup, repeated reload, empty-bank
recovery, local/signed/budget/alternative failures, unknown incoming consumers and
workbook/worksheet graph rejection. Final staged M4 acceptance is separate.
