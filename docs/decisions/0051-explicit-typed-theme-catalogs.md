# ADR 0051: Explicit typed theme palette and font catalogs

## Decision

`WorkbookReader::read_theme_catalog` returns an owned canonical `ThemeCatalog`
without changing ordinary opaque `theme()` behavior or materializing worksheet
cells. The catalog contains source names, twelve colors in spreadsheet-index
order, major/minor Latin/East Asian/complex-script typefaces and ordered script
mappings. RGB letter case, empty typeface names, optional classification bytes
and system-color identity are retained. System fallback colors are document
values; no host color lookup is performed.

The explicit codec checks DrawingML namespace and parent context, validates XML
through EOF, enforces font record limits and rejects duplicate selected fields.
Unknown unrelated sections stay in the original serialization. Selected color
choices other than srgbClr/sysClr and transforms return Unsupported; they are
not silently converted to an incorrect RGB result. Complete theme/drawing
mutation and color-transform rendering remain staged.

Typed output is caller-owned and uncached. Preparation uses the remaining
aggregate metadata input allowance and the theme allowance after opaque payload
accounting. A conservative linear ledger charges lexical attribute bytes and
actual vector-capacity growth before retaining further records. Row/cell sizes
and ordinary read allocations remain unchanged. Returned caller-held catalogs
are outside subsequent reader retention. There is no implicit full theme DOM.

## Evidence

Two integration tests cover the public-generated default palette/font values,
strict/transitional namespaces, entities/case/empty names, foreign namespace
spoofing, missing themes, repeated reads, owner destruction, limits, duplicates,
invalid RGB and unsupported transforms. Failures retain usable original bytes.

Rust 1.88 workspace tests and latest-stable Clippy pass. The release example and
[scaling evidence](../../benchmarks/alpha5-theme-catalog.md) verify 0, 1,000 and
100,000 script records without worksheet decoding or temporary string stores.
This checkpoint does not close M2 or claim theme mutation support.
