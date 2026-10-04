# Upstream sources

Status: cloned and inspected in part; selected calamine numeric/package parsing has been ported and refactored. Exact symbols, destinations, notices, changes, and tests are in [ports.json](../third_party/ports.json). Selected rust_xlsxwriter writer layouts and umya-spreadsheet rich run/font/color layouts have also been adapted. Local source repositories live under `/workspace/upstream/` and are not vendored or runtime dependencies.

| Project | Repository | Pinned commit | Package version | Metadata license |
|---|---|---|---|---|
| calamine | https://github.com/tafia/calamine | `0af05f4f6030351e3b8a999ea0810c8618368776` | 0.36.1 | MIT |
| umya-spreadsheet | https://github.com/MathNya/umya-spreadsheet | `1bcf4d0be5b666e5b9788244f14b55ce72fa4f3f` | 3.1.1 | MIT |
| rust_xlsxwriter | https://github.com/jmcnamara/rust_xlsxwriter | `902609b2e0fbc3cfa20fec5b464328479e29dbef` | 0.99.1 | MIT OR Apache-2.0 |

A package version does not prove the checkout equals the crates.io release. Reproduce source with commit IDs. The existing benchmark uses crates.io calamine 0.36.1 rather than an assumed equivalent clone.

```sh
 git clone https://github.com/tafia/calamine.git calamine
 git -C calamine checkout 0af05f4f6030351e3b8a999ea0810c8618368776
 git clone https://github.com/jmcnamara/rust_xlsxwriter.git rust_xlsxwriter
 git -C rust_xlsxwriter checkout 902609b2e0fbc3cfa20fec5b464328479e29dbef
```

Initial inspection covered README, package metadata, licenses, and module names. Subsequent implementation-start inspection covered calamine's cell stream, workbook metadata, relationships, coordinate/value parsing, and writer memory-mode documentation. This is not a completed full-source audit.

## Provenance records required when porting

For each port record project URL, commit, source file/symbols, destination modules, chosen license, original notices, semantic changes, source tests, and feature inventory entries. Preserve copyright/license notices in distributed source or accompanying notices; this URL list alone is insufficient.

## Additional candidates

- umya-spreadsheet: cloned and pinned for selected rich run/font/color layouts. Rich metadata codecs use shared core models, bounded parsing and disk placement; additional existing-file feature modules remain candidates.
- zip, quick-xml, date utilities, tempfile: normal foundational dependency candidates.
- openpyxl: feature and interoperability reference, with architecture based on docs/module names. Important pending MR diffs were reviewed within the user's limited exception; see [review](openpyxl-mr-review.md) and pinned snapshots.
