# ADR 0025: Canonical cached number-format classification

## Decision

NumberFormat owns its source ID, literal code and cached date/duration classification. Fields are private; new and set_code keep classification consistent, while id/code/date_kind provide borrowed or copy access. Changing a code preserves the source ID. Clone retains independent owned spelling and consistent classification. This pre-1.0 Rust API adjustment does not require a binding to adopt Rust method names.

The existing allocation-free first-section classifier moves from the XLSX writer helper into the canonical core number-format module. Reader and writer use the same rules for quoting, escapes, elapsed brackets and dates. Imported styles directly binary-search sorted source records and read their cached classification; they no longer allocate a second catalog-sized declared-format scratch vector. Source overrides of built-ins remain higher priority; unknown locale codes are not invented.

Cell-format date lookup remains a bounded indexed table charged before allocation. Individual parser events, owned catalogs and dependency overhead retain their existing contracts; this is not a hard process RSS cap or a claim that all remaining preparation scratch concerns are resolved.

## Evidence

Core tests verify classification after code mutation and independent snapshots, with quoted/escaped/numeric formats. Existing source override, style/date validation, writer and interop tests remain passing. Workspace tests, formatting and warning-free Clippy pass. Large unused declared-format catalogs expose the eliminated scratch cost; measurements and the small observed wall regression are in benchmarks/m2-style-classification.md. No milestone completion is claimed.
