# Architecture

Update this file only when the architecture changes.

## Workspace

| Crate | Responsibility |
|---|---|
| `crabxl-core` | I/O-free models: values, addresses, formulas, styles, the editable workbook bank, worksheet storage, feature, drawing, chart and pivot models, errors and resource limits. |
| `crabxl-xlsx` | ZIP and OOXML: package and relationship graph, readers, writers, the source-preserving editor and the loaded workbook coordinator. |
| `crabxl` | Public facade. Re-exports and composes; no second engine. |

Dependencies flow `crabxl` → `crabxl-xlsx` → `crabxl-core`. The core never
imports XLSX code or a language runtime. Bindings (the Python package) depend on
`crabxl` only.

## Modes

| Mode | Owner | Memory |
|---|---|---|
| Streaming read | `WorkbookReader` row stream | Metadata, I/O buffers and one bounded row batch. Never a full worksheet. |
| Sequential write | `WorkbookWriter` and interleaved per-sheet spools | One row plus bounded buffers; rows spool to temporary files. Header and tail metadata are spliced at finish. |
| Owned workbook | `Workbook` bank | Sparse cells under an explicit memory allowance. |
| Loaded workbook | `LoadedWorkbook` over `WorkbookEditor` | Source parts stay in the ZIP. Cells load per sheet on demand; edits are overlays. |

## Shared model

- One value, formula, style, address and error model for every reader and
  writer. Style registries hand out typed IDs; readers adopt source catalogs
  without renumbering.
- Feature models (tables, rules, validation, comments, printing, protection,
  names, properties) are typed. Drawings, charts and pivots keep ordered
  `XmlElement` trees so every property and extension survives editing.
- `PartGraph` keeps relationship ownership separate from properties.
  `BinaryPayload` shares immutable memory or an owned disk source.

## Saving a loaded workbook

The editor writes only affected parts. Unchanged parts are copied with their
compressed bytes. Affected worksheet XML is rewritten as a stream: known
elements are replaced in schema order and unknown elements pass through.
Relationship, content-type and identity allocation happen in one transaction.
Markup that an edit would affect but CrabXL does not understand returns an
error instead of being dropped.

## Memory

- Buffers and batches are bounded by bytes and by row or cell count.
- Shared strings choose memory or a disk index with a bounded cache.
- Loaded models, overlays, feature snapshots and shared strings share one
  retained allowance. `AutoMemory` derives it from available memory (cgroup
  aware on Linux).
- Small metadata (styles, features, names, drawings) loads fully.

## Ports

Code adapted from calamine, rust_xlsxwriter and umya-spreadsheet is listed in
[third_party/ports.json](../third_party/ports.json). Pinned source revisions are
in [upstream-sources.md](upstream-sources.md).
