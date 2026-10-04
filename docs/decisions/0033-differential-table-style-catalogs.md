# ADR 0033: Canonical differential and table/pivot style catalogs

## Decision

StyleCatalog now owns source-ordered sparse DifferentialStyle components and an optional TableStyleCatalog containing explicit defaults and named region definitions. Differential components reuse the same Font, Fill, Border, Alignment, Protection and NumberFormat models/codecs; no parallel style engine is introduced. Differential number-format IDs/codes are independent of cell-format declarations.

All 28 pinned public table/pivot region tokens are typed. Elements retain optional size/differential IDs, order and duplicates. Named TableStyle.count retains source absence/spelling-independent numeric meaning, including large advertised counts; it never controls allocation. The containing list count is derived from actual records. Differential references validate after all style sections are consumed, so source section order need not precede references with definitions.

Byte/count limits account for component boxes, strings, gradient capacity/validation scratch, table/default names and nested element/vector capacities. New codecs share the existing reader Budget. Source adoption adds these costs to the existing cached registry ledger and respects caller caps without allocating by advertised counts or referenced IDs. Ownership transfer preserves existing payload allocations.

Differential extensions/unknown component markers remain explicit. New-package creation rejects unmodeled content before spooling, while the original-package editor can preserve unchanged source style XML. Typed access to extension payloads, worksheet conditional-formatting/table instances and their structural graphs remain staged. Compatible/RetainExplicit alignment serialization policies continue to apply; retained source properties and reference save representation are distinct concerns.

## Verification

Tests cover forward/sparse differential references, defaults/names, partial components, large optional count values, zero element sizes, invalid references/region tokens, duplicate roots, extension markers, tight-budget cleanup, pointer-preserving adoption and typed export/readback. Public constructors/serializers generate fixtures; public differential/table classes verify every selected component property and all 28 regions after both engines save. No additional reference implementation is inspected. See benchmarks/m2-style-extras.md and inventory checkpoint styles.differential-table-definitions.

This checkpoint does not complete M2/M4/M5 or expose a complete Python style interface. Definition read/create/export is separate from worksheet feature graph support.
