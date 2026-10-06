# Architecture

Status: M0 architecture/inventory and M1 raw numeric streaming and M3 sequential writer acceptance complete; M2 core acceptance is complete (see docs/validation/alpha5-m2-acceptance.md). The goal is a standalone Rust crate with full public openpyxl feature coverage, improved processing speed, and controlled memory consumption. An optional Python compatibility adapter is now authorized for shared tests; other language adapters follow the priorities and contracts in [binding-contract.md](binding-contract.md). Reader ownership is recorded in [ADR 0001](decisions/0001-numeric-streaming.md); the sequential writer uses [ADR 0004](decisions/0004-sequential-scalar-writer.md).

Source-backed scalar/formula append coordinates canonical model and package
overlays atomically; see [ADR 0063](decisions/0063-atomic-loaded-row-append.md).
Python exposure and remaining M4 sheet/structural mutations stay separate gates.

Canonical cells use bounded ordered blocks to reduce full-model tree allocation;
see [ADR 0064](decisions/0064-packed-sparse-cell-storage.md) and
[numeric/text tradeoffs](../benchmarks/alpha7-packed-models.md). Resource ledgers
remain conservative; measured RSS is reported independently.

Plain shared-string payloads use immutable shared ownership for retained models
and owned storage for ordinary row streams; see
[ADR 0065](decisions/0065-mode-aware-shared-text.md). Adaptive sampling uses the
ownership strategy of materialization to preserve its aggregate budget plan.

Namespace resolver updates occur only at declaration-bearing elements while
checked element nesting remains separate; see
[ADR 0066](decisions/0066-declaration-only-namespace-stack.md).
The semantic classification is now an exhaustive compact enum; snapshot/Result
layout changes and small mixed performance effects are recorded in
[ADR 0071](decisions/0071-compact-namespace-classification.md) and
[paired measurements](../benchmarks/alpha7-compact-scope.md).

Failure context is indirectly owned so successful Result paths do not carry its
full inline size; public error categories, diagnostics and source chains remain
unchanged. See [ADR 0067](decisions/0067-compact-error-context.md).

The shared checked XML event method inlines into codec loops after measured
numeric improvements; [evidence and code-size tradeoffs](../benchmarks/alpha7-inline-xml.md)
remain separate from Python conversion and calamine acceptance.

Packed model range iteration resolves each block's exact binary boundaries
without scanning preceding cells; see
[ADR 0068](decisions/0068-binary-sparse-row-boundaries.md) and
[public-core row measurements](../benchmarks/alpha7-row-cursor.md).

Source-backed title changes preserve immutable source selectors and stable bank
IDs without decoding cells; see [ADR 0069](decisions/0069-lazy-source-sheet-renaming.md).
Renamed lazy models hydrate with their current display title while retaining
original part identity, value overlays and unrelated package content.

Loaded display order uses a bounded source-index permutation and original sheet
declaration cache, with atomic bank/package updates and deferred active-index
semantics; see [ADR 0070](decisions/0070-source-sheet-display-order.md).
Affected local defined-name owner graphs remain an explicit M5 dependency.

Source-backed row/column and range transformations use canonical sparse edits
and borrowed imported-catalog row encoding; see
[ADR 0072](decisions/0072-source-backed-cell-structure.md). The first successful
structural operation retires that sheet's coordinate overlays; future edits and
append use its bank model. Affected unimplemented feature graphs reject before
mutation, while unrelated original assets remain repeatable source parts.

## Reference scope

The initial openpyxl architecture review used directory/module names and documentation only, without reading implementation or reconstructing internal call graphs. The user later authorized narrowly reviewing important pending merge-request diffs and necessary surrounding code. Findings are in [the MR review](openpyxl-mr-review.md).

- Architecture reference checkout: Mercurial `52c77fdee169`, default branch.
- Compatibility baseline: openpyxl 3.1.5, tag revision `13627b03ca25a1a98becf40e533b955615b13429`; the public-surface inventory is mapped against this version; see features.json and the reproducible catalog tools.
- Sources: README, development, optimized modes, performance, formula, date/time, pivot, and feature documentation.
- Documentation: https://openpyxl.readthedocs.io/en/stable/ . Default-branch documentation may differ from the release baseline.

## openpyxl feature boundaries

| Boundary | Reference packages | Responsibility |
|---|---|---|
| User-facing model | workbook, worksheet, cell | Workbook, sheets, cells, ranges, properties |
| Read/write coordination | reader, writer, workbook/worksheet codecs | Loading, saving, and worksheet modes |
| File format | packaging, xml | OOXML parts, relationships, content types, XML |
| Types and utilities | utils, descriptors, compat | Coordinates, dates, type descriptions, Python compatibility |
| Styles and rules | styles, formatting, worksheet feature modules | Formatting, validation, filtering, tables |
| Advanced content | chart, drawing, chartsheet, comments, pivot | Charts, images, comments, pivot data |
| Formula tools | formula | Tokenization and reference translation; not a calculation engine |
| Tests | tests within feature packages | Module-level behavior and interoperability fixtures |

The public documentation distinguishes a general editable model, lazy read-only iteration, and sequential write-only output. These boundaries are useful; Python descriptors and compatibility scaffolding do not need one-to-one Rust equivalents.

## Design decisions

- Preserve the complete public feature baseline. Staged implementation changes delivery order, not final scope.
- Use umya-spreadsheet as the primary feature-port source for editable workbooks,
  existing-file integration and M5/M6. Retain calamine and rust_xlsxwriter as
  performance references and secondary sources for overlapping read/write
  algorithms. Clone, pin, inspect and refactor coherent modules into the shared
  core; do not re-export upstream engines or maintain competing model banks.
- Use mature foundational ZIP, XML, date, and temporary-file crates as normal dependencies.
- Share a single value, formula, style, address, error, and feature model across readers and writers.
- Internals may use ownership, borrowing, enums, traits, iterators, builders, and typed IDs extensively. Public Rust names and syntax may differ, but functionality and observable semantics must meet openpyxl expectations.
- The Python adapter must use openpyxl-compatible calls and observable behavior; other adapters can map language-specific behavior later; they must not have to reconstruct missing core spreadsheet capabilities.
- Correctness, memory use, and performance are joint acceptance criteria. Avoid unnecessary allocation, copying, and per-cell metadata duplication, and measure relevant changes.

## Workspace

Independent append-only worksheet spools share one writer style bank and package
creation order while retaining bounded per-sheet buffers. Header-only declared
dimensions support optimized consumers; see ADR 0046.

```text
crabxl/
├── Cargo.toml
├── crates/
│   ├── crabxl/              # Public Rust facade and prelude
│   ├── crabxl-core/         # Shared types, editable model, feature models
│   └── crabxl-xlsx/         # ZIP/OOXML, readers, writers, preservation
├── tests/fixtures/
├── benchmarks/
├── third_party/               # Port provenance and required notices
└── docs/
```

The three workspace crates now exist. Tests currently generate small OOXML fixtures in memory rather than storing a binary fixture directory. Dependencies flow from facade to XLSX to core; the facade also exposes core types. Start with modules and split additional crates only when useful.

```mermaid
flowchart TD
    A[Future language adapters] --> F[Public Rust API]
    F --> X[XLSX reader / writer / round-trip]
    F --> C[Core models / values / features]
    X --> C
    X --> P[OOXML parts / relationships]
    P --> Z[ZIP I/O / XML events]
    X --> S[Strings / styles / dates]
```

### Core

- Values and addresses: empty, number, text, boolean, spreadsheet error, date/time semantics; typed zero-based row/column indices with explicit A1 parsing and formatting.
- Formulas: text, typed cached value, shared/array/dynamic metadata; tokenizer and translation later.
- Styles: shared style tables and typed IDs; no separate incompatible reader/writer style systems.
- Editable model: sparse Workbook/Worksheet/Cell storage, names, merged ranges, sheet properties, and structural editing.
- Feature models: tables, filters, validation, conditional formatting, comments, drawings, charts, pivots, protection, document properties.
- Batches: values separated from optional metadata, actual row/column positions, owned batches and useful borrowed views.
- Errors and limits: one matchable error vocabulary with part/location context and explicit resource budgets.

### XLSX

- Package: archive access, content types, relationship graph, normalized part resolution; never assume sheet1.xml or a fixed workbook path.
- XML: event-based decoding/encoding, namespaces, escaping, shared number/text rules; no full-sheet DOM.
- Metadata, strings, styles, dates: decode and encode the same core types, loading only what is needed.
- Reader: row/batch streaming and explicit full-model loading built on the same cell decoder.
- Writer: streaming output and editable-model output using the same encoders and feature serializers.
- Parts: feature-specific decoders/encoders separated from public models.
- Round-trip: original part inventory, dirty tracking, reuse of unchanged parts, and synchronized relationships/content types.

### Public API and future adapters

Separate WorkbookReader, WorkbookWriter, and editable Workbook capabilities. Use concrete types, Result, fallible iterators, and builders rather than dynamic arguments. Keep I/O independent of foreign runtimes and document index conventions, ownership, close/finish/abort, cancellation, and cleanup. Provide safe owned handle/batch paths alongside borrowing; do not introduce unsafe or a stable C ABI merely for hypothetical bindings. Declare only supported Send/Sync properties.

## Port allocation

The following allocations describe the overall port plan. Numeric package/cell/coordinate parsing is the first completed subset, traced in [ports.json](../third_party/ports.json). Remaining allocations are candidates. Exact source revisions are recorded in [upstream sources](upstream-sources.md).

| Upstream | Destination | Refactoring |
|---|---|---|
| umya-spreadsheet workbook/worksheet/package | Loaded coordinator and shared editable models | Primary source: reuse feature and relationship rules, replace unbounded XML/model copies with borrowed or budgeted sources |
| umya-spreadsheet common/advanced feature families | Shared M5/M6 models and XLSX read/create/edit codecs | Primary source: styles, tables, validations, comments, drawings, charts, pivots and metadata; verify actual support, preserve provenance |
| calamine datatype/formats | Core values, XLSX date/style codecs | Shared value and format semantics |
| calamine xlsx/mod.rs and cells_reader.rs | Package metadata, reader, strings, part codecs | Reuse cell parsing; emit rows/batches; move Range collection to explicit materialization |
| calamine errors/utils | Core errors/addresses, XLSX helpers | One coordinate/error convention; remove duplication |
| calamine vba.rs | Macro metadata and preservation | Reuse relevant XLSX behavior without importing unrelated formats |
| rust_xlsxwriter workbook/worksheet | Core models and XLSX writer | Separate model, validation, encoding, I/O; port constant-memory path |
| rust_xlsxwriter format/styles/datetime/formula/shared strings | Shared models and codecs | Unify reader/writer representation and IDs |
| rust_xlsxwriter packager/relationships/content types/XML writer | XLSX package/XML | Shared part graph and serialization rules |
| rust_xlsxwriter charts/drawings/images/tables/rules/notes | Core feature models and XLSX parts | Pair serializers with readers and editing support |
| Upstream tests/examples | Focused regression fixtures and benchmarks | Retain provenance; validate this project's public behavior |

Port coherent modules, retain mature algorithms, and integrate them into one
architecture. This priority does not imply all umya features match the pinned
public baseline or its I/O/memory strategy should be copied unchanged. Prioritize
measured read/write speed and peak RSS costs alongside functional integration.

## Memory strategy

The user requires both intelligent automatic allocation and explicit memory budgets, with advanced tuning. More available RAM should enable measured faster strategies, such as bounded caches, larger useful batches, indexing, or independent-sheet parallelism; low RSS is not the objective by itself. The first numeric policy is implemented in [ADR 0002](decisions/0002-adaptive-memory.md): scan streams; repeated access samples and materializes if its estimate fits the policy allowance, with budget fallback. It accounts for effective Linux availability, mounted cgroup v1/v2 visible hierarchies, process constraints and headroom, with explicit budget and advanced overrides (ADR 0044). Incomplete discovery falls back conservatively; hidden ancestors and private desktop job/process constraints remain limitations. Native Windows/macOS host probes and caller-controlled measured concurrency are verified in ADRs 0050/0053. `ResourceLimits` and operation budgets are not a global process RSS ceiling. `input_buffer_bytes` remains explicit because its larger settings showed little benefit. `read_sheet` explicitly materializes a bounded numeric snapshot through the same decoder; this is separate from the later full editable workbook model.

Read ZIP metadata, select a worksheet, stream decompression/XML events, decode selected cells, and produce bounded rows/batches. Validate entry lifetimes without solving ownership by buffering an entire worksheet.

- Bound buffers/batches by bytes and cell/row count; retain sparse positions and expand only explicitly requested finite rectangles.
- Budget shared strings; large-string completion requires a disk-backed index and bounded cache, not only an out-of-memory-limit error.
- Account for metadata and styles; they are not inherently constant-sized.
- Editable models may scale with loaded data, while lazy parts, shared IDs, and dirty tracking reduce duplication.
- Port the writer's constant-memory/temporary-file approach, document mode-specific restrictions, and measure temporary storage and I/O. Full feature coverage must remain available through appropriate modes.
- Numeric streaming targets metadata plus I/O buffers plus one row/batch rather than memory proportional to total rows. User-retained batches are additional memory.

Initial Rust source inspection confirms calamine already has a low-level cell reader. Its Range collection and eager workbook-level strings/styles still require separate analysis. rust_xlsxwriter's documented constant-memory mode flushes rows to temporary files and restricts editing earlier rows; its low-memory mode still keeps unique shared strings in RAM.

## Complete round-trip

Readers plus writers do not automatically provide safe modification of existing files. Preserve original part inventory, unknown subtrees, namespace context, binary content, and references. Rebuild affected parts only, synchronizing styles/strings IDs, formulas, relationships, and content types. Opaque preservation is a separate capability from reading/creating/editing a feature. Reject unsupported destructive edits rather than silently dropping content. Signed-document behavior needs an explicit policy.

## Benchmark evidence

The earlier feasibility benchmark compared openpyxl and calamine only. Current crabxl measurements use a reproducible three-scale numeric experiment with checksums, raw runs, wall time, and kernel peak RSS; see [benchmark evidence](../benchmarks/README.md). Results apply to raw numeric streaming, not the future full-feature library or bindings.

## Initial editable/preservation implementation

M4 now separates an I/O-free sparse `Worksheet` from a lazy `WorkbookEditor` holding original parts and bounded cell overlays. Both use the shared core values and existing XLSX codecs. See [ADR 0005](decisions/0005-sparse-preserving-editor.md) for ownership, budgets, cache invalidation, atomic path output and unsupported edits. This first checkpoint does not complete M4 existing-file structural editing.

## Optional Python adapter

the [standalone Python repository](https://github.com/crabxl/crabxl-python) is a standalone Cargo package in the Python repository and Maturin/PyO3 package. Python objects and compatibility naming live there; the three core Rust crates do not depend on Python. The adapter targets openpyxl call compatibility and is staged by capability. [ADR 0006](decisions/0006-python-compatibility-adapter.md) records the test and ownership contract.

## Owned workbook bank

[ADR 0007](decisions/0007-owned-workbook.md) adds the I/O-free shared Workbook bank, stable sheet IDs, aggregate mutation allowances and borrowed writer export. Existing-package preservation retains its lazy editor path. Python active selection uses the common XLSX metadata codec; Registered new Python Workbook sheets now use aggregate bank ownership through stable owned handles; loaded models remain in the original-package path with per-model/overlay allowances.

The owned workbook bank now owns an optional canonical immutable shared Theme. Aggregate mutation allowances include its conservative holder charge; writer export shares its bytes after preflight rather than copying serialized payloads. This does not yet integrate complete loaded style/theme catalogs or typed DrawingML edits. See ADR 0026.

Canonical style registries can adopt validated reader catalogs by ownership transfer, retaining source IDs, duplicate component slots and raw cell-format flags. Shared core reference validation serves format readers and owned registration. Sparse source number-format IDs use bounded reservations rather than dense storage. Full bank/export integration follows this foundation; see ADR 0027.

Source style export shares canonical tables/registration and uses explicit derived automatic date IDs instead of a fixed imported-table prefix. Raw source formats retain referenced components without copying font/gradient payloads. Source custom format declarations remain sorted after insertion while preserving numeric IDs. New-package creation rejects unmodeled sections; preserving original graphs stays with the lazy editor. See ADR 0028.

Shared finite font/gradient domain validation and selected color identity semantics are verified through public constructor/save/reload behavior. Gradient preparation reserves both retained stop capacity and validation scratch; existing event and catalog budgets remain separate. See ADR 0029.

The owned bank now contains an optional canonical StyleRegistry, with source ID adoption and full/raw/code registration under aggregate sheet/theme/catalog allowances. WorkbookParts owns the original entry iterator and registry indices; consuming writer export moves these without catalog/sheet snapshots. Styled borrowed export fails early until a separately budgeted non-consuming path is implemented. This is new-package creation, not loaded original-graph preservation. See ADR 0030.

Differential formatting reuses canonical style component types/codecs in sparse optional boxes. TableStyleCatalog owns explicit defaults and source-ordered named region definitions, with all 28 region tokens and deferred differential-reference validation. Shared byte/record budgets count nested vector capacities, component boxes and strings, never advertised counts. Unmodeled differential extensions reject new-package export. Worksheet rule/table graphs remain separate M5 integrations. See ADR 0033.

Bounded preview consumers can opt into `ReadOptions::stop_after_last_row`.
With explicit row bounds this stops after the selected prefix and deliberately
does not validate the unread worksheet tail or its ZIP CRC. The default remains
a full-part scan and validation. Each independent stream owns its parser state.

Exact charset/theme/indexed style identities share StyleInteger and the existing
ExactInteger representation. Small identities allocate no payload; large owned
identities are included in font/color/fill/border/differential/registry budgets.
Serializers borrow non-Copy color records. See ADR 0054.

Lazy source-backed `LoadedWorkbook` now moves source styles into the canonical
bank and commits full-sheet models behind stable identities, with joint source/
model/SST accounting (ADR 0056). The same source coordinator now integrates
preserving value overlays with lazy or cached models and repeatable original-part
saves (ADR 0057). Python loaded handles now share this same owner in
[adapter ADR 0017](https://github.com/crabxl/crabxl-python/blob/f3251f63c33626338c9c071b5df515ca199bd688/docs/decisions/0017-canonical-loaded-workbook-owner.md).
Lazy active selection rewrites workbook views independently of value-cache
invalidation (ADR 0058). Other source graph mutations remain in progress.

[ADR 0060](decisions/0060-shared-sheet-visibility.md) shares visible/hidden/very-hidden
state across owned models, source catalogs and preserving loaded metadata edits.
These edits do not decode worksheet cells and retain original caches and assets.
All-hidden output fails before writing; loaded active normalization commits only
after successful serialization. Python exposure and broader M4 sheet surgery
remain staged for Alpha 7.

[ADR 0061](decisions/0061-deferred-active-view-selection.md) centralizes relative
and deferred workbook-view indexes and compatible visibility normalization.
Bindings consume the canonical policy; source active declarations, pending
views and loaded stable handles remain coordinated without cell materialization.

Source-backed sheet creation installs bounded live catalog membership instead of
another workbook model; see [ADR 0073](decisions/0073-live-source-catalog-membership.md).
New bodies share the borrowed row encoder; original bodies remain lazy and
repeatable. Catalog copy/removal extend this transaction in ADRs 0074/0075; deferred
feature graphs remain open.

Supported loaded worksheet copies duplicate only the requested canonical model
and retain a compact immutable source-template index for borrowed repeat saves;
see [ADR 0074](decisions/0074-source-backed-worksheet-copy.md). Unsupported affected
graphs reject before registering the copy; unrelated sheets remain lazy.

Loaded worksheet removal transfers one requested model and atomically retires its
package identities and pending edits; surviving copies keep their immutable source
template. Shared checked incoming-edge inspection guards both worksheet removal
and calculation-chain disposal; see [ADR 0075](decisions/0075-source-backed-worksheet-removal.md).
Workbook identity graphs outside worksheet relationships remain explicit M6
structural dependencies, while unrelated preserving edits remain available.

Source-backed physical-cell removal transfers the removed owned cell through
`LoadedWorkbook::remove_cell`. It retains the logical append extent and shares
structural-edit graph validation and joint resource reservations. A missing cell
returns `None` without committing a model rewrite or invalidating source formula
caches; temporary reservations are released through the normal rebalance path.

Linux Auto availability now recognizes clean inactive file cache within visible
cgroup v1/v2 limits, conservatively excluding dirty/writeback pages and retaining
parent/host/process and policy constraints; see
[ADR 0076](decisions/0076-reclaimable-cgroup-cache.md). Missing cache statistics
retain the earlier raw-headroom estimate. Explicit budgets remain independent.
