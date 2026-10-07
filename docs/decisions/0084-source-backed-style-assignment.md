# Source-backed style assignment

Status: native implementation verified; A9 Python integration and publication pending.

Loaded workbooks expose `set_style` for existing canonical IDs and `set_number_format` for derived formats. Derivation preserves font/fill/border/base/alignment/protection identities and flags. Supported original sheets materialize once and reuse guarded model saves. Existing IDs reject before materialization; affected unsupported feature graphs, signed sources and unknown style sections reject before format registration.

A dirty stylesheet is serialized at its relationship-resolved original path from the canonical bank. Unchanged styles remain original compressed parts. Byte-bounded output and aggregate model/style reservations remain shared with existing save/load paths. Adding a stylesheet to a source without one is still pending. Successfully registered formats may remain reusable if a subsequent cell edit fails.

Explicit temporal style assignment records a numeric-encoding preference inside the existing temporal payload. It does not change date value equality or clone the value. Source ISO dates not explicitly restyled retain the previous ISO fallback; assigning General explicitly instead writes a numeric serial. Reapplying the same numeric-cell style stays a no-op; a first explicit temporal assignment may change its serialization preference even when the style ID is unchanged.

Existing loaded tests verify invalid-ID rejection before materialization, temporal identity, repeated General date exports, new number-format registration, source component identity, empty styled cells and repeated stylesheet saves. The original ISO-date structural regression remains passing. Core Rust 1.88 tests, all 44 writer tests, loaded tests and strict workspace Clippy pass.

[Release probe evidence](../../benchmarks/results/alpha9-style-assignment-loaded-foundation.json) verifies one million numeric cells after assignment and a finalized 100,000-cell General-date export. Direct assignment median was 0.0581 seconds versus 0.2168 seconds for clone/replace; peak RSS was approximately 33 MiB for both. Export median was 0.0970 seconds, peak process RSS 8,440 KiB including untimed read-back. This is the retained native probe, not a loaded-style benchmark or a published-version comparison; temporary storage was not measured in it. Loaded performance and Python acceptance remain A9 gates.
