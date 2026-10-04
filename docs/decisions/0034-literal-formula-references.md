# ADR 0034: Literal formula references with explicit geometry

## Decision

FormulaReference owns either validated worksheet bounds or a literal public property. FormulaRange remains a type alias for existing construction calls. Literal references retain empty, absolute-axis, qualified and opaque strings in a single Box without allocating failed geometry parses. The fallible range() accessor resolves physical geometry only when an operation requires it; set_range() validates before replacing obsolete spelling.

Array/table creation and source decoding retain reference and input strings rather than requiring worksheet-local A1 coordinates. DataTableOptions.validate_inputs() is an explicit geometry check, not a construction precondition. XML/entity validation and writer preflight still reject illegal XML characters. ValidateGroups shared-template reads require real declared bounds; Compatible expansion uses the actual first definition and does not interpret unused range hints.

Public probes show that both array and data-table empty references are omitted on save, even though the assigned property retains an empty string. Compatible serialization matches this behavior; RetainExplicit keeps the explicit empty attribute. Reloading an omitted array reference produces None. The previously recorded pinned-reference missing data-table reference reload defect is not reproduced as a canonical reader crash.

## Verification and remaining work

Core tests cover borrowed spelling, cloning, byte accounting, explicit geometry failures and validated replacement. Codec tests cover opaque array/table/shared references, empty source inputs, strict shared-group rejection with part context and both output policies. Public constructors/serializers and readback verify nine records against canonical creation. See benchmarks/m2-literal-references.md for interoperability and streaming regression evidence.

This does not implement formula evaluation, arbitrary shared-index properties, unknown formula-header compatibility or structured-formula graph transformations. M2 and M4 remain in progress.
