# ADR 0031: Optional structured formula expression ownership

## Decision

The canonical Formula model owns an optional boxed expression. Existing literal/source constructors create a present expression, and expression() retains its borrowed XML-body view (absent becomes empty). with_optional_expression permits explicit absence only for array/data-table formulas; optional_expression exposes presence for language adapters. Normal/shared-template literal validation remains unchanged. The optional expression owns no text payload when absent; this is a semantic correction, not an allocation or performance optimization claim.

Absence and an explicit empty literal differ before serialization. Both write an empty formula body; source reading returns a present empty expression. This matches observed ArrayFormula(None, text=None), text="" and text="=1" public assignment/save/reload behavior. Missing reference remains an independent optional metadata property already supported by core. No cache is fabricated.

Public openpyxl 3.1.5 DataTableFormula(ref=None) construction/save is valid, but normal reload raises TypeError because a required constructor argument is absent. Record this reference defect; do not introduce a native panic/crash to copy it. This checkpoint does not broaden the physical range model to arbitrary literal reference strings or implement array_formulae mapping.

## Verification

Core tests distinguish absent/empty/source expression presence, clone equality, optional metadata and invalid normal optional-expression construction. Writer/readback tests verify absent reference, empty source body and absent cache. Public generated XML records and three array readbacks match, including data-only None values. Shared-template read regression measurements remain separate from literal presence semantics. See benchmarks/m2-optional-formula.md.

The Python adapter must map its text=None property to this canonical presence field rather than retaining a second formula engine or view-only cache. Adapter integration is a separate pinned checkpoint.
