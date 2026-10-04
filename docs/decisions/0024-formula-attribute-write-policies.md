# ADR 0024: Formula attribute compatibility and explicit retention

## Decision

WriteOptions and EditorOptions expose FormulaWritePolicy. Compatible is the default: newly assigned false optional flags and empty data-table input strings are omitted, matching public reference save/reload behavior. Source FormulaFlag spellings remain verbatim, including nonempty strings "0" and "false"; these are observable reference properties rather than newly assigned booleans. RetainExplicit writes owned false flags as "0" and retains empty input attributes as a Rust extension. Owned values remain unchanged before saving under either policy.

Core DataTableOptions permits an explicit empty input string, while every nonempty coordinate still validates. Both sequential creation and original-package replacement use the same formula serializer; untouched source parts retain their existing preservation behavior. Flags outside the reference wrapper remain available to the Rust core. This does not implement formula calculation or spill/shared graph surgery.

## Evidence

Writer tests distinguish owned/source flags and both policies. Editor tests verify the same behavior through repeated saves. Public constructor/save/reload comparisons check all data-table properties and their boolean/string/None distinctions, including empty input strings; the native and public records agree. Workspace tests, formatting and warning-free Clippy pass. See benchmarks/m2-formula-attributes.md for interoperability and shared-reader trend measurements.
