# ADR 0017: Canonical component registration and style serialization

## Decision

StyleRegistry owns the same StyleCatalog used by readers, with separate collision-checked indices for fonts, fills, borders, custom number formats and complete cell formats. CellStyle is a convenient owned registration input, not a second retained workbook style bank. Registration moves new payloads into canonical tables, reuses existing component IDs and deduplicates formats. A borrowed format key avoids allocating an alignment wrapper for duplicate calls. Equal positive/negative zero values hash alike; missing and explicit optional attributes remain distinct.

Registration validates common numeric/component invariants in core, shared with imported and rich styles. XML-specific text validation remains in XLSX. Actual vector capacities, boxed/name/gradient payloads and conservative hash/control/index storage are bounded. A payload/index-capacity ledger makes retained accounting constant-time rather than rescanning all records during every registration. A separate audit verifies the ledger against actual catalog capacities. Table capacities grow geometrically when the allowance permits, falling back to a single required slot under tight budgets. Every affected table/index is reserved before committing logical records; failed registrations preserve identities and contents, while successful capacity reservations may remain accounted for reuse. Accounting is managed storage, not exact RSS or allocator overhead. Caller-owned input is additional; gradient validation scratch is bounded before allocation.

The sequential writer uses this registry, including two required fills and its established five cell-format IDs. Builtin codes reuse portable IDs; custom codes share declarations. Phonetic references are checked against actual font-table indices rather than the count of cell formats. The catalog and registration indices release on abort. Row streams expose a borrowed prepared catalog so callers can inspect values and styles without copying payloads or materializing sheets.

Font names, number-format codes, named-style labels and pronunciation attributes use the shared streaming XML attribute escaper; no whole escaped copy is built. A generated 70KB start attribute demonstrates explicit event limits: the default 64KB reader event cap rejects it contextually, while a configured 128KB cap reads the original attribute exactly. A writer metadata allowance is not a guarantee that every independently configured reader accepts the output.

## Public behavior

Default cell font/alignment components match public workbook defaults: Calibri 11, family 2, minor scheme and theme slot 1, with absent alignment overrides. Public default style readback compares all common font, fill, border, alignment and protection properties and named-style labels.

StyleWritePolicy::Compatible omits zero/false alignment attributes, matching public reference save/load behavior. In particular literal false wrap/shrink values reopen as None, while rotation/indent/reading-order public defaults remain zero. Core registered values retain their literal identity before serialization. RetainExplicit is a distinct Rust extension preserving optional alignment attributes. Color/protection and full source-record identities remain modeled; original-package untouched styles preserve source XML independently. Public source observations are under docs/research/style-zero-serialization-public.json.

## Evidence and remaining work

Core collision, signed-zero, bounds, ledger and failure-atomicity tests and XLSX component sharing, font-reference, escaped-attribute, default and round-trip tests accompany public default/complete-style readback. Representative creation measurements compare normalized registration, exact prior core revision 3dfda8a and pinned public sequential creation, with every requested style/value checked. Both native workflows perform an additional full duplicate-registration pass; they do not have identical API call counts to the public reference. Builds and unrelated checks are excluded during timing. See benchmarks/m2-style-registry.md and raw results.

Loaded bank catalog ownership, stable imported IDs during typed mutation, themes, complete differential/table-style editing, dynamic metadata and aggregate Auto allocation remain staged. No general existing-file style edit or M2/M4 completion is claimed.
