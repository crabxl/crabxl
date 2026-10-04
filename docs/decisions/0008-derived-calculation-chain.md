# ADR 0008: Derived calculation-chain policy for existing-file edits

Status: accepted for conventional content-type overrides and supported OPC relationship graphs.

## Decision

A calculation chain is derived ordering information, not a formula calculation engine or canonical cell model. Original unchanged saves preserve it. Default edited saves discard it, invalidate worksheet formula caches and request automatic full recalculation. EditorOptions.calculation_chain can select CalculationChainPolicy::RejectEdits to retain the earlier explicit rejection behavior.

Catalog chain paths from content-type overrides, resolve strict/transitional workbook relationship types through the shared OPC helpers and inspect incoming package relationships through bounded XML events. Retain only a small path inventory; never load chain cells, worksheet XML or a relationship DOM. Relationship scans share an aggregate metadata byte cap and CRC-check through EOF. Inventory paths count toward conservative metadata allowances, separately from managed patches and process RSS.

On an edited save, omit chain parts and empty companion relationship parts, remove the corresponding content-type overrides and workbook relationship declarations, and retain all unrelated identities/parts. SaveStats.removed_parts reports omissions. Account for removed uncompressed bytes before rewriting package totals. Path output remains adjacent-temporary and atomic after ZIP completion; repeated saves use the original source, and clear_edits restores the original chain on subsequent unchanged output.

## Preservation boundaries

Unknown incoming consumers, outgoing chain extension relationships, markup-compatibility alternatives in scanned relationships, externally linked chains, missing/misdeclared typed targets and unsupported graphs must not produce dangling references. Reject edits when safe removal cannot be established; unchanged output remains available for supported parseable metadata. The conventional override-based chain is verified; default-content-type chain discovery and more advanced graph transformations remain staged.

Digitally signed packages still allow unchanged preservation and reject edits. No signature is silently stripped or represented as valid after editing. Creating or repairing digital signatures is outside current openpyxl feature behavior and this checkpoint.

## Verification

Tests cover nonstandard chain paths, strict relationship types, default discard and explicit reject, synchronized metadata removal, unrelated binary/XML retention, repeat saves, revert, missing types, foreign consumers, outgoing extensions, namespace/alternative guards and bounded relationship scans. Shared public Python tests compare edited chain output against openpyxl. Release evidence includes formula/numeric readback, absent stale caches, time/CPU/RSS/temporary space and an ordinary no-chain editor regression.
