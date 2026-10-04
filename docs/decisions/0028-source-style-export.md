# ADR 0028: Source style export and derived date identities

## Decision

WorkbookWriter::from_style_catalog adopts validated source tables by ownership transfer. Existing font/fill/border/base/cell/named-style IDs remain stable. Unknown style sections/extensions are rejected before worksheet output; creating a new package is distinct from original-package preservation. XML text and shared model/reference validation apply before emission.

Automatic date style IDs are registered after existing records and stored explicitly in the value encoder rather than assumed to be 1 through 4. These presets derive from source format zero using shared component IDs; font names and gradient vectors are not copied to build them. An existing date-compatible style zero remains zero when serializing its date/clock values. New default writers retain the existing five initial IDs. Full arbitrary date/style assignment compatibility remains a separate acceptance concern.

Raw register_format and register_number_format methods use the remaining writer metadata allowance, including source declarations and sheet/theme catalog charges. Core variants accept smaller aggregate limits. New number formats keep declarations sorted and update internal hash positions when inserted among sparse source IDs, avoiding linear lookup fallback on each styled cell. Source IDs are not changed. The former 65,373-style constructor cap is replaced with the actual u32 identity/cardinality boundary and explicit caller byte/record limits.

Source unused declarations remain available in the Rust catalog/export, beyond the reference's pruned save representation. Output sizes and extra native serialization work are reported rather than treated as identical internal work. Bindings need not expose native table identities or retention extensions as reference APIs.

## Verification

Tests cover sorted sparse insertion with intact code indices, retained source format prefixes/components, source-zero dates, derived automatic IDs, typed date/clock/duration readback, early extension rejection and configured 100,000-record limits. Workspace tests, formatting and warning-free Clippy pass. Public save/reload verifies all ten values and specified font/fill/alignment/protection/format properties; representative default creation regressions are retained in benchmarks/m2-style-export.md.

Complete loaded-bank style mutation, arbitrary date/style assignment, typed differential/table/extension schemas and original structural graph editing remain required. No milestone completion is claimed.
