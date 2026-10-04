# ADR 0035: Visible normal/shared formula header compatibility

Compatible reads select formula category before interpreting hints, independent of attribute order. Unknown encoding types are treated as normal expression text, matching the pinned public reader. Normal formula hints are not value semantics; shared formulas require an index and retain a literal reference but ignore unrelated attributes. All attributes still undergo XML/entity decoding and individual byte checks before unused semantics are skipped. No payload is allocated for discarded hints.

ValidateGroups keeps the previous strict unknown-type/attribute/hint validation and shared geometry requirements. The package editor also keeps strict header decoding when analyzing replacement dependencies; value-reader compatibility does not authorize unsafe graph edits. Original untouched XML remains preserved by the editor.

Core/codec tests cover unknown types, attribute order, unused invalid hints, unknown entities, strict failures with part/cell context and unchanged ordinary/shared caches. Original public probes verify visible expressions and data-only values. Streaming measurements use an immediate preserved 5ecb5bd baseline, not a distant aggregate checkpoint; see benchmarks/m2-formula-headers.md.

This checkpoint is limited to normal/shared headers. Array/table raw flag strings, full unknown structured-header behavior and arbitrary public shared-index identities remain open. It does not finish M2/M4 or promise preservation of unused hints in newly created packages.
