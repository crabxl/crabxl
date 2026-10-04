# ADR 0032: Single-payload literal array text and XML body views

## Decision

Formula::from_array_text stores the original optional literal array property in one boxed string. FormulaMetadata's literal_array_text origin marker distinguishes this property from an XML expression body. Only array formulas may use it. No second formula expression, language-side text cache or duplicate engine is installed.

expression()/optional_expression() borrow the XML body by skipping the first Unicode character of an owned literal. This matches pinned public array-object save behavior even for non-equals prefixes. array_text() borrows the original literal for owned objects and formats a leading equals for source XML only on request; absence remains None. Ordinary Rust formula constructors keep their existing optional-equals normalization. from_source always clears the literal origin marker because its input is already an XML body, including when callers reuse metadata from an owned literal.

This deliberately distinguishes literal property access from source readback: None, empty and "=" all reload as "="; "abc" reloads as "=bc"; a non-ASCII first character is removed as one character rather than one UTF-8 byte. Physical range modeling and other formula gaps remain separate.

## Verification

Core tests verify absent text, borrowed literal access, clone spelling, single-character/empty/equals/double-equals/Unicode slicing, invalid non-array origin and source-body metadata reuse. Public probes compare eight formula XML records, ordinary readback and missing caches. Older optional-expression fixture/probe stays intact. Representative shared-template regression evidence is in benchmarks/m2-literal-array-text.md.

The adapter will call these canonical ownership/conversion methods instead of preserving literal spellings only in Python objects. No milestone completion or additional binding surface is claimed here.
