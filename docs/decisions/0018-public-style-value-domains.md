# ADR 0018: Public style value domains and hexadecimal spelling

## Decision

Public reference constructors and native output readback demonstrate fractional font families in the closed range 0 through 14, signed integer charsets and color theme/indexed identities, and case-sensitive hexadecimal color strings. Core Font.family is therefore an optional finite f64; charset and indexed/theme references use i64 rather than unsigned byte/slot types. Numeric identity is not proof that a slot resolves in the current palette. Resolution may return an unavailable result without rejecting a retained reference.

ArgbLiteral validates six/eight hexadecimal digits and stores numeric channels plus an eight-bit lowercase mask, without an owned string allocation. Six-digit inputs acquire a zero alpha prefix. Canonical uppercase input uses the existing ColorKind::Argb variant; mixed/lowercase input uses ArgbLiteral. Display and XLSX serialization preserve spelling. Hashing preserves the same identity as equality, including signed-zero normalization for fractional font families. Common validation rejects nonfinite/out-of-range families before mutation.

The signed numeric domains are bounded by Rust i64. Arbitrarily large Python integer metadata is not claimed here and remains an explicit compatibility boundary for later adapter acceptance. Raw theme/indexed identity retention does not add theme resolution or loaded style mutation.

## Evidence

Public constructor observations are in docs/research/style-domains-public.json. Native create/read tests verify fractional families, signed/large charsets, signed palette/theme references and mixed-case colors; public openpyxl readback verifies the same output. These observations use public APIs and generated files, not reference implementation source. The prior registry workload is rerun with an exact preserved previous executable to measure the expanded records; see benchmarks/m2-style-domains.md.
