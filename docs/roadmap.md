# Development roadmap

Status: M0 architecture/inventory and M1 raw numeric streaming acceptance complete; M2 is in progress with exact integers, boolean/error literals, and plain inline/shared text with owned-payload accounting and adaptive RAM/disk SST storage and explicit typed rich runs/phonetics; M3 sequential writer acceptance is complete for scalar/date/time/duration, normal formula/cache and basic-style creation. Selected calamine parsing logic has been ported into the three-crate workspace. [features.json](features.json) maps every pinned public surface and release documentation topic, while verified behavior remains a narrow subset. See [milestone evidence](milestones.md). Deliver the independent Rust crate while the optional Python adapter enables shared compatibility tests; other language adapters follow separately.

Consult [pending MR risks](openpyxl-mr-review.md) and [work-item, board, and milestone risks](openpyxl-work-items-review.md) when implementing affected features. Defects are regression cases to fix or avoid, not behavior that compatibility requires reproducing.

See [binding priorities, interface principles and performance targets](binding-contract.md). Python is tier 1, followed by JS/TS on Node and WASM sharing one ExcelJS-compatible API, then .NET and Java/Kotlin, PHP and Go, and finally Ruby, Swift and Dart. Rust core remains canonical; compatibility interfaces and language-specific extensions evolve separately. Faster-than-openpyxl performance is required; faster native overlap and lower RAM targets are desired, with equivalent-workload evidence.

Worksheet printing metadata (margins/options/setup/page properties and sparse breaks) now has typed read/create/edit support through a selected rust_xlsxwriter port (ADR 0043); printer graph mutations, header/footer creation, areas/titles and protection remain open.

Worksheet viewport metadata now has typed read/create/edit support through a selected rust_xlsxwriter port (ADR 0042); workbook/chartsheet/custom views and the remaining common/advanced feature families stay open.

See the [detailed Rust completion plan](completion-plan.md) for dependency order, remaining checkpoints, resource strategies and acceptance gates.

## Scope and completion

The compatibility baseline is openpyxl 3.1.5, Mercurial tag revision `13627b03ca25a1a98becf40e533b955615b13429`. The version-specific inventory uses public runtime metadata and release documentation without reading implementation source. It covers 190 modules, 567 public classes and their public members, functions/exports, and 45 RST documents. This is an audit index, not a complete behavioral specification; semantic verification is staged. Partial support and preservation-only features in that baseline must also be represented.

For each item record reference documentation, source modules, read/create/edit/preserve capabilities, supported modes, tests, limitations, and milestone. Use planned/ported/verified status. Missing features stay in the roadmap; neither upstream availability nor benchmark results justify removing them.

Internal design can be deeply idiomatic Rust. Public function names and syntax may differ, but core data and behavior must fulfill the same capabilities. The optional Python adapter uses openpyxl-compatible calls now; other binding adapters map language-specific calls later. An Excel calculation engine is not part of openpyxl's existing formula support.

## Initial feature inventory

| Group | Required inventory | Initial source or gap | Milestones |
|---|---|---|---|
| Files/workbooks | XLSX/XLSM/XLTX/XLTM, new/open/save, stream I/O, sheets/order/visibility/active sheet, epochs, names, calculation properties, external links, core/custom properties, protection | calamine metadata + writer models; template/external-link/editing gaps | M1, M4, M5 |
| Cells | Numbers, strings, booleans, empty, errors, date/time/duration, hyperlinks, rich text, formula/cache metadata, ranges, merges, row/column access | Shared value/format/formula models; rich-text and metadata gaps | M1-M4 |
| Editing | Random access, append, insert/delete/move/copy, dimensions, sparse layout, outline/groups/hidden, freeze panes, views, sheet properties | Core editable model; preserve baseline structural-edit behavior without inventing automatic reference-update guarantees | M4, M5 |
| Modes | General editable, streaming read, streaming write; mode-specific capabilities, values-only, projection, row/column iteration | Refactored calamine reader + constant-memory writer + full model | M1-M4 |
| Styles | Font/fill/border/alignment/number format/protection, named styles, themes/colors, sharing/deduplication, row/column styles | Writer styles + reader formats; full style parser gap | M2, M3, M5 |
| Formulas | Normal/shared/array/dynamic metadata, tokenizer, A1 translation, cached results, data-table formulas | Partial Rust sources; additional parser/translation work | M2, M5 |
| Tables/rules | Tables/styles/structured references, filters/sort conditions, validation, all baseline conditional rules | Writer serializers; corresponding readers/editors | M5 |
| Comments | Baseline comments/authors/size/position and preservation limits, notes/VML | Writer modules + readers/common models | M5 |
| Images/drawings | Supported formats, anchors, position/size, relationships, preservation/editing | Writer modules + readers/round-trip | M6 |
| Charts | Area/bar/line/pie/doughnut/scatter/bubble/radar/stock/surface, relevant 3D types, combined charts, axes/series/labels/layout, chartsheets | Audit writer coverage; missing chart types/readers/editing remain required | M6 |
| Pivots | Baseline pivot/cache/record reading, preservation and editable settings | Major gap; source evaluation or new implementation | M6 |
| Printing/protection | Print areas/titles, setup/margins/headers/footers/breaks, sheet/workbook protection | Writer modules + readers | M5 |
| Extended data | VBA preservation, macro relationships, external-link caches, unknown extensions, dynamic-array metadata and cm/vm references | Source modules + full part graph; pending MR regressions | M4, M6 |
| Data integration/utilities | Public coordinate/date utilities and row data exchange supporting future dataframe adapters | Rust rows/batches and optional Serde; language integrations deferred | M2, M7 |

Extra upstream features may be retained as extensions but do not replace baseline gaps. XLS/XLSB/ODS are calamine formats outside this openpyxl compatibility scope and may be separate future format adapters.

## M0: Sources, inventory, and shared design

1. Pin upstream clones and license information. Fix the openpyxl release baseline and finish the detailed public-feature inventory.
2. Inspect Rust source, APIs, and tests. Use openpyxl documentation and public behavior for architecture; narrowly review important MR diffs only within the user's exception.
3. Evaluate additional sources such as umya-spreadsheet only for concrete parser/round-trip gaps; check licensing and memory behavior before selecting code.
4. Establish the workspace, MSRV/edition, shared values/styles/errors/addresses/limits, and documented API conventions.
5. Record ZIP ownership, reusable calamine cells_reader logic, and constant-memory writer restrictions in short architecture decisions.

Acceptance: reproducible sources, traceable inventory, and one shared model; do not introduce two incompatible workbook systems first.

## M1: Port streaming numeric reads

Current checkpoint: relationship-based workbook/sheet discovery, sparse numeric/empty rows, strict/transitional namespaces, projection, reusable rows, owned batches, explicit numeric sheet materialization, configurable input buffers/data budgets, and cleanup/error tests. The initial numeric Auto policy now selects a useful mode from access pattern, availability, bounded sampling, and enforced retained-data allowance. This is raw numeric reading; style-zero formatting is not interpreted. Results and reproduction commands are in [benchmarks](../benchmarks/README.md). Further adaptive caches/concurrency and later value/style semantics remain open.

- Extract package/metadata/cell parsing from calamine. Make row/batch output the streaming path and keep Range materialization separate.
- Resolve workbook/sheet parts through relationships. Handle multiple sheets, sparse coordinates, incorrect dimensions, namespaces, malformed input, resource release, and error context.
- Apply projection using the current cell coordinate before expensive decoding, avoiding the stale-counter risk in MR !454.
- Benchmark 10,000/100,000/1,000,000 rows by 10 columns against fixed calamine and openpyxl read-only versions; warm up once and alternate five measured runs.

Acceptance: correct counts/checksums, raw timing/RSS results, numeric streaming memory that does not grow linearly with total rows at fixed buffer settings, and no full worksheet/XML buffer. This is a prototype, not a feature-complete release.

## M2: Complete value semantics and large strings

- Extend the initial numeric Auto policy to strings/styles/caches and measured concurrent strategies; add native probes beyond Linux cgroup v2 while preserving the portable caller-availability override. Account for effective availability, headroom, workload, and measured benefits. Retain managed-allocation diagnostics and do not claim a hard global RSS limit without enforcing all dependency allocations.
- Extend raw M1 `f64` numbers to preserve the baseline's integer/float distinctions and exact integer literals where required; document representation and overflow behavior without silent precision loss.
- Port inline/shared strings, booleans, errors, rich text, dates/number formats, formula text, and cached values into the common representation.
- Test both date systems, absent cached results, shared/array formula metadata, whitespace-only text, and empty runs.
- Add disk-backed shared-string indexing, bounded cache, byte limits, cleanup, and temporary-storage metrics.
- Provide Rust iterators, owned batches, and resource handles suitable for future adapters without implementing a binding.

Acceptance: numeric, repeated-string, and high-cardinality-string correctness/memory tests. Rejecting all large string files at a budget limit is not complete large-file support.

## M3: Port writers and shared-model round-trip (complete)

- Extract writer packaging, XML/cell encoding, initial styles, and constant-memory output from rust_xlsxwriter.
- Use shared values/styles/formulas/addresses/errors and relationship/content-type/ID management.
- Support numeric/text/date/formula/basic-style output. Distinguish finish, close, abort, and Drop cleanup; do not fabricate numeric empty cached results.

Acceptance: cross-tool readback and OOXML relationships, mode restrictions, timing/RSS/temp space/output size, and resource cleanup on errors/abandonment.

## M4: Editable model and existing-file preservation (in progress)

First checkpoint: budgeted sparse core cell/range operations and borrowed writer export; lazy original-part inventory, unchanged compressed passthrough, existing-cell overlays, global cache invalidation and repeatable atomic path saves. [ADR 0005](decisions/0005-sparse-preserving-editor.md) and [release evidence](../benchmarks/m4-editor.md) document verified boundaries. The next checkpoint adds an owned workbook/sheet aggregate with stable IDs, guarded total allowances, explicit copies, order/active selection and borrowed export; see [ADR 0007](decisions/0007-owned-workbook.md). Remaining M4 work includes loaded-model aggregate integration, existing-file structural edits with feature/reference handling, and broader part/reference handling. The derived calculation-chain discard/reject policy and signed-package edit rejection are explicit; advanced graph cases remain staged. M2 full style/date/string catalogs also remain required.

- Implement sparse random access, append, copy/insert/delete/move, lazy parts, dirty tracking, and original part inventory.
- Rewrite affected parts with synchronized IDs and preserve unchanged or unknown content and namespace context.
- Preserve macro/template files and explicitly track read/create/edit/preserve as separate capabilities.
- Keep image sources safely reusable across saves without requiring duplicate resident bytes for all images; preserve caller-owned resources.

Acceptance: unchanged round-trip, single-cell edits, structural edits, repeat saves, and no silent loss of unrelated content. Rewritten XML need not be byte-identical, but semantics and references must remain valid.

## M5: Complete common workbook and worksheet features

- Complete styles/named styles/themes, rich text/hyperlinks/views/dimensions/outlines/protection.
- Add tables, filters, validation, conditional formatting, comments, print settings, names, document properties.
- Complete tokenizer/translation/formula metadata; model whole-row/column ranges without dense allocation.
- Pair each serializer with reader/model/edit/round-trip tests. Distinguish valid x14 support from opaque extension preservation.

Acceptance: all assigned inventory entries verified, remaining gaps explicit, preservation not substituted for edit support.

## M6: Drawings, charts, pivots, complex parts

- Complete anchors/images/drawings/chartsheets/chart types/axes/series/layout/combined charts, including baseline types absent from the writer source.
- Add pivots/caches/records, external relationships, macro/unknown extensions, metadata indices/cm/vm, dynamic arrays, and rich-value preservation.
- Evaluate additional source projects for specific gaps using the same provenance and shared-model policy.

Acceptance: real-file fixtures with feature/reference assertions; a file opening successfully alone does not establish full preservation. Claim completeness only when all baseline capabilities pass.

## M7: Rust crate stabilization

- Finalize API documentation/examples, MSRV, features, version/error policy, and supported platforms.
- Benchmark read/write/edit across numeric/text/styled/multi-sheet workloads, recording RSS and temporary storage.
- Complete formatting/lint/tests/docs and license checks.

Acceptance: a directly usable Rust crate with the full verified baseline matrix. Language adapters are a separate later project.

## Evidence policy

Use source tests to preserve behavior, integrate common models, decouple I/O, refactor streaming, and then measure. Add failure cases for known defects before changing behavior. Keep timing benchmarks separate from deterministic correctness tests; do not claim unmeasured memory improvements.

## Parallel Python compatibility acceptance

The user moved a thin optional Python adapter ahead of milestone completion to share tests and ease migration. Workbook/Worksheet/Cell calls must match openpyxl; Rust internals and public Rust naming remain independent. Selectively reuse pinned upstream tests with exact assertions/provenance, then expand the tested surface along M4-M7. Unsupported behavior stays explicit and planned, including read-only/write-only binding modes, full style/date/string reading, complete formula tokenization, loaded structural edits and advanced features. This does not complete M4-M7 or the whole Python compatibility suite.

M5 translation checkpoint: the bounded shared A1 scanner and atomic translated sparse moves are implemented and exposed through compatible Python Translator/move_range calls. All 46 selected original worksheet/translator methods pass. Full tokenizer/formula metadata and other M5 feature families remain required; M4 existing structural edits are also unfinished.
