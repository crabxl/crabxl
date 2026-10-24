# umya-spreadsheet as the primary feature source

## Decision and scope

The user selected umya-spreadsheet as the primary source for remaining editable
workbook/package and M5/M6 ports. Existing calamine and rust_xlsxwriter work stays
useful for overlapping read/write algorithms and performance comparisons.
Retain one canonical CrabXL model and codec layer. Source priority does not mean
silently wrapping multiple engines or adopting upstream behavior without checking
the pinned openpyxl public contract.

The initial source decision deferred umya to individual missing modules. Rich
text and style components were subsequently adapted, with their exact provenance
in `third_party/ports.json`. The original decision did not include a measured
comparison with its complete editable core. It therefore did not establish that
combining separate reader/writer sources would be cheaper than adapting umya.

This assessment uses stable 3.1.0, tag commit
`aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b`, whose declared MSRV is 1.88. Exact
crates.io versions are used in the independent benchmark. Development commit
`baca6c37361975315d4424fab8160b4e04453770` declares 3.1.1/MSRV 1.89 and is not a
stable-release substitute. Historical rich/style ports keep their original pin.

## Inspected architecture

The README support table lists XLSX/XLSM read/write, lazy loading, cell/style and
row/column editing, worksheet creation/copy, drawings, images, charts and OLE.
This is substantially broader implemented feature coverage than current CrabXL;
it is not evidence of complete openpyxl parity or preservation of every graph.

- `reader/xlsx.rs`: shared strings and styles load before worksheet access.
  `lazy_read` defers cell models. It also offers
  `read_sheet_by_name_stream`, a per-cell callback API.
- `structs/raw/raw_worksheet.rs::read_lazy` calls
  `RawFile::set_attributes_from_source`, which in turn calls `set_attributes`.
  `structs/raw/raw_file.rs::set_attributes` reads an entire uncompressed part
  into a `Vec<u8>`. Adding the source path does not clear that vector. The
  callback route calls `lazy_read`, so a streaming callback alone does not prove
  bounded XML retention. Measure this cost before selecting the I/O strategy.
- `structs/shared_string_table.rs` retains strings in a vector and hash map.
  Disk-backed SST policies and aggregate caller controls require adaptation.
- `structs/cells.rs` stores boxed cells in a hash map with two coordinate tree
  indexes. Cells contain separate boxed value/style fields. This favors several
  query directions but has a different per-cell allocation cost from CrabXL's
  sparse row-ordered model. The public `Worksheet::cells` returns a reference
  vector; this is additional traversal allocation, not a clone of cell payloads.
- `writer/streaming_writer.rs` flushes complete worksheets. It is not by itself
  equivalent to append-only row spooling with bounded memory for one huge sheet.
- `writer/xlsx.rs`: path output streams a ZIP through a temporary file; arbitrary
  non-seekable writer output uses `make_buffer` for the complete ZIP. Do not
  mischaracterize every save as an in-memory ZIP. Saving clones the stylesheet;
  raw and deserialized worksheets use different serialization routes.
- `reader/driver.rs::xml_read_loop` panics on XML errors. User-input errors must
  become contextual fallible errors when adapting this control flow.

Concrete feature ports should start with workbook/worksheet/package declarations
and relationship/content-type ownership, then common and advanced families.
Preserve mature layouts and algorithms, record symbols and MIT notices, remove
redundant models, and verify read/create/edit/preserve separately. Avoid bringing
whole-part vectors, per-cell style clones or panic paths into bounded routes.

## M6 work reusable from this source

| Acceptance family | Inspected candidates | Remaining integration or gaps |
| --- | --- | --- |
| Images, drawings and anchors | `structs/anchor.rs`, drawing marker/anchor families, `structs/image.rs`, `reader/xlsx/drawing.rs`, `writer/xlsx/drawing.rs`, media and relationship codecs | Canonical shared types, repeatable borrowed/disk-backed binary sources, relationship IDs, Python calls, structural anchor updates and bounded I/O |
| Charts | `structs/chart.rs`, `structs/chart_type.rs`, `structs/drawing/charts/*`, paired chart and drawing readers/writers | Verify each baseline type/combination, unsupported extension cases, shared styles/formula references, loaded editing, chart-sheet model and Python conversion |
| Pivots and caches | `structs/pivot_table*`, `structs/pivot_cache_definition*`, paired table/cache readers/writers | Reuse definition schemas and layouts; current cache-record handling retains raw bytes, not a complete typed record editor. Shared cache ownership, bounded records and dependent consumers need additional work |
| External links and complex metadata | Raw relationships, macros, cell metadata index and extension candidates | No complete chartsheet/external-link/dynamic-array/rich-value graph implementation established by this inspection. `cm` is read but its writer emission is commented out; `vm`/metadata part integrity need independent verification and implementation |

`reader/xlsx/pivot_cache.rs` copies original cache records to a vector;
`writer/xlsx/pivot_cache.rs` clones that vector before output. These are useful
preservation paths, not proof of typed cache-record read/create/edit support or
appropriate ownership for large caches. `structs/drawing/extension_list.rs`
contains an unimplemented parser. Do not mark these acceptance groups complete
from module names, opaque byte retention or a successfully opened ZIP.

The source can remove substantial M6 schema/codec implementation work. Full M6
still requires matching the complete baseline, integration with source-backed
edits and shared relationships, relevant deferred M4 cases, Python APIs and
verification across repeated saves. No percentage or line-count savings is
claimed without mapping actual symbols to every acceptance case.

## Compatibility observation

The public `new_file` + numeric cells + `writer::xlsx::write` workflow emits
`cellXfs/xf` records with `xfId="0"` but no `cellStyleXfs` table. CrabXL's current
style-reference validation rejects that output as a missing reference. The
benchmark independently verifies numeric XML output; this does not claim CrabXL
can currently reopen every umya-created file. Determine the appropriate
compatibility/base-style policy and add interoperability coverage rather than
normalizing the fixture to hide the gap.

## Performance evidence and limits

`benchmarks/editable_engines.py` compares CrabXL, calamine 0.36.1, umya 3.1.0 and
rust_xlsxwriter 0.99.1 using one native release executable and explicit mode
labels. Read streams, calamine noneditable ranges, complete editable models,
initialization-only lazy routes, full-model edit/save and creation are distinct
workloads. Every output numeric coordinate/value is verified outside timing;
time, kernel RSS, output size and working-file sampling are reported separately.
Generated numeric inputs do not represent high-cardinality SST, styled/formula
models or the user's unavailable NYC 1M-by-41 file.

Independent 500 Hz gperftools sampling of the initial two-million-cell stream
recorded 321 samples: the XML stream event route was present in roughly 77% of
samples. A million-cell sequential write recorded 226 samples: compression was
present in roughly 86%. These small diagnostic samples guide optimization and
are not exact isolated percentages or portable performance guarantees. The
profiling instrumentation is excluded from release timing runs.

Use the measurements to remove repeated parsing, allocation and serialization
work, then compare identical prior/current operations. Do not reduce compatible
semantics, namespace validation or resource accounting to improve a headline.
Compression-level changes require explicit output-size and throughput tradeoffs.
