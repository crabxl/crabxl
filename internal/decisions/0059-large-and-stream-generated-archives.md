# Large and stream-generated archives

Archive byte sizes are not retained-memory estimates. Legal large worksheets
must not require resource tuning merely to pass fixed compressed, aggregate
uncompressed, or worksheet-size thresholds. These three defaults are now
`u64::MAX` (no application size cap). Callers can still configure finite caps;
managed memory, metadata, XML event, row, and batch allowances remain independent.

The ZIP dependency's `decompressed_size()` returns `None` for entries using data
descriptors, even when their central-directory sizes are available. Alpha 5
incorrectly classified this result as an exceeded archive limit. When callers
configure a finite aggregate cap, inspect entry sizes from the central directory
without decompressing payloads. Report actual and configured sizes in errors.

The existing archive-limit test covers streamed ZIP and ZIP64 descriptors,
successful value decoding beyond an incorrect `A1` worksheet dimension, and
explicit finite-limit acceptance and rejection. This does not constitute a
measurement of the reported NYC million-row workbook.

Python read-only iteration follows the declared worksheet dimension, matching
the public compatibility baseline. For a producer that incorrectly declares
`A1:A1`, call `ws.reset_dimensions()` before iterating. A preliminary
`calculate_dimension(force=True)` is only necessary when the bounds themselves
are needed; it performs an additional scan.
