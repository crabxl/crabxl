# Buffered original-package XML compression

The lazy editor emits many small XML fragments through quick-xml. Feeding each
fragment directly into the ZIP compressor dominated numeric edit/save time.
Rewritten parts now use a fixed 64 KiB BufWriter before the compressor. This
buffer fits the existing 64 KiB operation working reserve; it is not retained
worksheet XML, a new spool or a cache that grows with input size.

PartOutput continues counting and limiting every accepted XML byte before
buffering. Each rewrite explicitly flushes before returning its byte count, so
deferred compression/output failures are reported before another ZIP entry is
started or the archive is published. Path saves retain their adjacent temporary
output and atomic replacement behavior. Existing output-failure/retry, part-limit,
formula-cache, shared-string, calculation-chain, view and printing tests exercise
the same rewrite routes.

Value edits still invalidate caches in all worksheets and request recalculation.
Unchanged entries still use compressed passthrough. This change does not infer
formula dependencies, bypass XML/CRC validation, or alter original-package
preservation. Benchmark evidence compares every uncompressed output part against
alpha.2; see benchmarks/alpha3-edit-buffering.md.
