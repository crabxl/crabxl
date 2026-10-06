# ADR 0077: Borrow complete scalar cells from the XML input buffer

Status: implemented; Python integration and A8 release acceptance remain open.

Numeric profiling identified repeated XML event dispatch and per-cell attribute
validation allocation as dominant costs. The existing reader now recognizes a
complete unprefixed `c/v` pair already present in its bounded input buffer. It
accepts only numeric, Boolean and shared-string ID cells with distinct `r`, `s`
and `t` attributes. A fixed three-slot attribute classifier avoids allocating
duplicate-check storage; unknown or duplicate attributes fall back to the
checked event parser. Both paths use the same attribute, numeric, Boolean,
shared-string, style and date decoders and canonical cell model.

Recognition requires the existing spreadsheet namespace and row depth, a
balanced lexical pair, ASCII numeric content and all applicable event, cell and
depth bounds. The reader never skips a pending expanded empty-element end.
Prefixes, declarations, entities, CDATA, formulas, inline strings and unusual
content retain the normal parser. Consuming recognized bytes through
`Reader::stream()` preserves byte offsets, decompressor accounting and CRC
verification. No worksheet XML or extra unbounded buffer is materialized.

The implementation is original CrabXL code. quick-xml 0.42.0's Reader stream,
buffered-event and expanded-empty-element behavior was inspected to verify the
consumption invariants; no new upstream implementation was copied. Existing
namespace-port provenance remains in `third_party/ports.json`.

Existing streaming tests cover tiny buffers, namespaces, CRC, styles, strings,
dates, formulas and resource bounds. Their existing malformed/order tests now
exercise balanced scalar duplicate attributes, reversed/duplicate coordinates,
failed-value statistics and event-size boundaries through both parsing paths.
No timing thresholds or additional test functions are introduced.

Paired release measurements include complete value checks and retained-model
construction. Numeric and repeated shared-text models beat calamine at the
reported large scales, but unique shared text remains slower and numeric stream
RSS remains higher. These are measured subsets, not general read-performance
acceptance. See [measurements](../../benchmarks/alpha8-buffered-scalars.md).
