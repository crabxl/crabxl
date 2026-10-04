# ADR 0038: Shared temporal number-format variants

DateKind supplies the pinned public default codes for date, datetime, clock and elapsed duration. Writer automatic presets use these codes rather than extra fractional-format extensions. Values retain their existing serial precision independently of display code. The clock default reuses portable built-in 21 when no source declaration overrides that meaning.

A temporal value preserves any existing date/duration format, even when its display interpretation differs from the original temporal kind. Otherwise an explicit style gains a number-format variant sharing font/fill/border IDs and preserving alignment, protection and other format properties. Applying the new number format is explicit. Writer input rows remain borrowed and are not rewritten; output cells reference the interned variant. Owned-bank pre-save style-property integration remains separate M4/M5 work.

StyleRegistry.find_format_with_number_format() and registration use the existing borrowed FormatKey and collision-checked index. No extra variant cache or second deduplication table is introduced. Existing variants require no alignment clone or retained growth; only a new format record owns a cloned small alignment. Number-format/component IDs and all imported records remain stable, including source built-in overrides. Global writer metadata allowance includes newly derived records and indices.

All row inputs validate before style derivation. Invalid XML/value/coordinate inputs leave row bytes and style identities unchanged. A later encoded-size/output failure can retain a valid reusable interned variant, accounted against metadata limits, but commits no worksheet row; retries can reuse it. Low-budget registrations return typed errors and preserve logical IDs.

Tests cover borrowed lookup equivalence, repeated tight-budget reuse, new-record rejection, component sharing, input atomicity, four defaults, escaped/literal non-date formats and retaining different temporal display categories, including a date-formatted duration loading as a datetime. Twenty public value/style cases verify both engines' readback. Streaming creation and immediate prior writer regression are recorded in benchmarks/m2-temporal-styles.md.

This checkpoint does not complete style assignment APIs, loaded style graph editing, all source extension cases or M2/M4/M5. No additional reference implementation is inspected; public constructors and saved/reloaded properties establish behavior.
