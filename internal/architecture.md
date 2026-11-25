# Architecture

Update this file only when the architecture changes.

## Workspace

| Crate | Responsibility |
|---|---|
| `crabxl-core` | I/O-free models: values, addresses, formulas, styles, the editable workbook bank, worksheet storage, feature, drawing, chart and pivot models, errors and resource limits. |
| `crabxl-xlsx` | ZIP and OOXML: package and relationship graph, readers, writers, the source-preserving editor and the loaded workbook coordinator. |
| `crabxl` | Public API: `Workbook`, `read::Reader`, `write::Writer` and their values, styles, references and features. Composes the engine; no second engine. |

Dependencies flow `crabxl` → `crabxl-xlsx` → `crabxl-core`. The core never
imports XLSX code or a language runtime. Bindings build their native API on
`crabxl`; compatibility interfaces that need engine-level access (the openpyxl
adapter) also use `crabxl-xlsx` and `crabxl-core`.

## Public API

| Type | Engine |
|---|---|
| `Workbook` | `LoadedWorkbook` over a boxed source. A new workbook starts from an empty in-memory package. |
| `read::Reader` | `WorkbookReader` row streams. |
| `write::Writer` | `WorkbookWriter` interleaved per-sheet spools. |

Worksheet handles borrow their workbook, one at a time. References are A1 text
or zero-based `CellRef`, `RangeRef` and `Span` values.

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

Saves are lossless by default. Codecs note content their typed models do not
represent (`loss::note`); a save re-reads the source of every block it
rewrites under `loss::track`, and every place that would remove content asks
the `DataLossPolicy` first (`loss::remove`). Path saves go through
`write_atomically`.

## Structural edits

`StructuralEdit` (core) maps indexes, spans, ranges, formulas, features,
anchors, chart series and defined names. `LoadedWorkbook` plans every update
before the cells move, so content that cannot follow rejects the edit while the
workbook is unchanged.

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
