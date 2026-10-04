# ADR 0026: Canonical bank theme ownership

## Decision

Workbook owns an optional canonical Theme alongside the sheet bank. set_theme atomically checks the same aggregate retained allowance used for sheet creation/copies/mutations. Rejected replacement keeps prior bytes, sheet identities and values. Sheet mutation allowances subtract theme storage; clearing a theme releases that charge for later work.

Theme uses immutable shared byte ownership. Clone copies a holder, not the serialized payload; dropping the original does not invalidate remaining holders. memory_bytes conservatively charges the full payload, container and reference counts to each budget holder even when another holder shares it. Initial Box-to-shared conversion may transiently retain the input while allocating; caller inputs and allocator overhead remain additional. No process RSS guarantee is implied.

Borrowed write_workbook export retains a shared theme holder, preflights combined style/theme metadata and respects explicit Validated mode before starting any sheet. A custom model theme uses compatibility emission by default; an absent model theme leaves the writer's configured default/custom/omit choice intact. Untouched loaded-package editing and typed palette/font resolution are separate capabilities.

## Verification

Tests cover shared byte identity/lifetime, atomic rejected replacement, aggregate sheet/copy limits, budget reuse, opaque export and strict validation before temporary writes, including retry after rejection. Workspace tests, rustfmt and warning-free Clippy pass. Native snapshot measurements verify every byte in every holder after source drop; representative style-writer regression remains recorded. See benchmarks/m4-bank-themes.md.

This is owned theme integration, not complete loaded-bank/style editing or M4 acceptance.
