# Configurable ZIP compression

WriteOptions and SaveOptions accept compression_level: Option<u8>. None retains
Deflate level 6; 1 through 9 select Deflate levels; 0 uses ZIP Stored entries.
zip 8.6 rejects explicit Deflate level 0 despite its API documentation describing
0..9, so uncompressed output uses its supported Stored method instead.

Shared validation rejects out-of-range values before writer construction or
editor output. WorkbookWriter.set_compression_level changes only packaging, not
spools or values. Editors apply the option to rewritten XML parts; untouched parts
retain the exact original compressed bytes. Limits, explicit compression-buffer
flushes, CRC/XML checks, global formula-cache invalidation and atomic path-target
replacement remain intact. Changing a level does not make unsupported editing
operations available.

The default Cargo feature is deflate-zlib-rs, selecting zip's
deflate-flate2-zlib-rs backend. The former zip deflate feature already selected
zlib-rs plus unused Zopfli support. Selecting zlib-rs explicitly removes that
unused backend; it does not claim a new default algorithm or a backend-only
speedup. Optional deflate-zlib selects native zlib and requires a C toolchain.
Cargo features are additive: use --no-default-features --features deflate-zlib
when selecting native zlib exclusively. This is a build-time choice, not a
per-workbook runtime switch. Neither backend adds handwritten unsafe code to
CrabXL; their dependency implementations remain outside its managed allocations.

Focused tests verify output semantics and CRCs across default, uncompressed,
fast, intermediate and maximum levels; preserved raw entries; invalid-option
target protection and retry. Rust 1.88 verification includes both backends.
Performance evidence measures numeric/mixed creation and existing-file edits,
including compression time, output bytes and peak RSS. The default level remains
6 to retain existing behavior; callers choose the time/size tradeoff explicitly.
