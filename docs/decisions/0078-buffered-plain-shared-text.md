# ADR 0078: Decode simple shared text from the bounded input buffer

Status: implemented; Python integration and A8 release acceptance remain open.

Unique-text profiling after ADR 0077 identified shared-string table parsing as
the largest remaining component. A guarded recognizer now handles a complete
ASCII `<si><t>...</t></si>` already inside the existing input buffer. It retains
the same namespace, depth, part, event, cell and entry limits and canonical XML
character validation. Entities, carriage-return normalization, attributes,
prefixes, rich runs, phonetics and larger/cross-buffer content use the original
rich-text parser. No whole SST XML or worksheet is materialized.

Shared-value mode creates its existing `Arc<str>` directly from validated
borrowed text, avoiding an intermediate Box and second payload copy. Owned-row
mode keeps fallible string-buffer reservation. Existing protection handling,
RAM/disk table placement, caches, accounting and cleanup remain canonical.
No second string table or additional public setting is introduced.

The buffered-element implementation is shared with scalar recognition and
inlined into its two bounded entry points. The scalar function retains the same
504 machine instructions in the measured native worker; this is not a claim
that compiler layout or total performance is invariant. The implementation is
original CrabXL code using inspected quick-xml 0.42.0 stream behavior.

Existing string tests now compare simple/empty/spaced/protected/entity and CRLF
content through one-byte and ordinary input buffers, and reject forbidden XML
characters. Existing CRC, entry/disk limits, rich modes and cleanup tests remain
applicable. Rust 1.88, full workspace tests, both compression backends, strict
Clippy and documentation checks pass without new test functions.

[Paired measurements](../../benchmarks/alpha8-direct-sst.md) demonstrate a large
unique-text improvement. Calamine times are close rather than establishing a
reliable universal speed advantage. Small numeric/styled/mode changes are mixed;
the desired numeric stream RSS target remains unmet.
