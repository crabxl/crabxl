# ADR 0027: Canonical catalog adoption and source-format edits

## Decision

StyleRegistry::from_catalog takes ownership of existing canonical tables without remapping component, cell-format or base/named-style IDs. Duplicate component records remain in place; new appearance registration reuses the first matching record. Number-format declarations are sorted by source identity, without changing their IDs. Existing IDs, including u32::MAX, use a bounded sparse sorted reservation vector rather than a dense table. New custom formats allocate unused IDs from 164 onward. An imported override of a built-in ID cannot silently change a newly registered literal built-in code.

Registry import validates shared catalog references and typed components, forecasts collision-checked index growth and accounts for retained capacities, payloads, index slots and reserved IDs. Gradient validation scratch is checked against the import allowance before allocation. register_format edits complete source format records against existing components without introducing appearance defaults; application flags, alignment/protection absence and explicit values remain distinct. Failed operations retain logical identities, while successfully reserved capacity may remain reusable and charged.

WorkbookReader::into_style_catalog consumes the reader and transfers its validated catalog without cloning source vectors/strings. Other archive/shared-string resources close; derived lookup vectors are discarded. Shared canonical number-format classifications remain in the transferred records. This enables later loaded-bank integration rather than keeping a second style engine.

## Verification and scope

Tests cover source vector identity, duplicate components, sparse maximum IDs, reused imported formats, built-in overrides, raw flag/absence-preserving edits, missing links, duplicate declarations and index allowance rejection. Reader and registry use shared core reference validation. Workspace tests, rustfmt and warning-free Clippy pass. Public model-call measurements verify all typed values and a source-format change; see benchmarks/m2-style-import.md.

This is canonical in-memory catalog adoption/editing. Complete workbook-bank style integration, imported catalog writer adoption, loaded structural/package edits and unmodeled typed style sections remain required. No M2/M4 completion is claimed.
