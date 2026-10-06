# ADR 0079: Borrow UTF-8 runs while scanning XML replacement bytes

Status: implemented; A8 integration and release acceptance remain open.

The shared cell/attribute encoder previously decoded every UTF-8 character to
locate XML replacements. Those replacements are ASCII, whose bytes cannot occur
inside a multibyte UTF-8 sequence. The encoder now scans bytes and writes the
same unchanged runs and replacement literals. Existing XML character validation,
text versus attribute rules, whitespace and carriage-return handling remain
unchanged. No unsafe conversion, buffer copy or additional dependency is used.

The existing primary writer round trip now includes multibyte characters directly
adjacent to replacement boundaries. Existing text/rich/attribute/style/printing
tests remain applicable. Rust 1.88 and both compression backends pass the writer
tests; full workspace, Clippy and documentation checks also pass. Existing
cell-layout provenance and notices remain
in `third_party/ports.json`, with this original integration adjustment recorded.

[Paired measurements](../../benchmarks/alpha8-byte-escape.md) show a modest gain
for the large Unicode/text workload and mixed small/numeric results. Complete
outputs pass public-reference readback and mode-specific uncompressed package
equality. This does not establish universal writer or editor superiority.
