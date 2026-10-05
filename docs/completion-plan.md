# Rust core completion plan

## Scope and progress rules

This plan completes the Rust core against the pinned openpyxl 3.1.5 public capability baseline. Python remains a separately versioned compatibility adapter and test client; other language bindings are subsequent projects. Formula calculation is not a baseline feature and is not introduced as a completion requirement.

M0, M1 and the scoped M3 sequential-writer acceptance are complete. M2 and M4 are in progress; M5 has a bounded A1 translation checkpoint. M6 and M7 are not complete. M3 completion does not imply complete strings, styles, formulas or file editing. Each remaining milestone must meet its whole acceptance definition before its status changes.

The current inventory mixes large public-module/document entries with narrow verified checkpoints, including two Python-specific entries. Raw item counts and completed-milestone counts are not effort percentages. Do not report either as a precise Rust completion percentage. Expand broad entries into behavioral acceptance cases while retaining their original baseline mapping; do not inflate progress by counting new micro-items.

For every feature track read, create, edit and preserve separately, with streaming-read, materialized/editable and sequential-write restrictions. Mark incompatible cases explicitly; a file opening successfully or an opaque part surviving does not verify typed access or editing. Completion evidence must link tests, relevant performance measurements, limitations and source provenance.

## Dependency order

1. Specify and implement M2 shared/inline text and bounded shared-string storage.
2. Implement read-side style/date catalogs and complete remaining M2 formula/value semantics.
3. Integrate catalogs and loaded worksheets into the existing canonical workbook bank, then finish M4 package and structural editing.
4. Implement M5 feature families with matching codecs and model edits.
5. Implement M6 drawing/chart/pivot and complex package graphs on the same models and package infrastructure.
6. Finish M7 portability, documentation, broad regression and performance acceptance.

Documentation, failure checks, interoperability tests, provenance and relevant benchmarks accompany each checkpoint. M7 starts during development; it is not an excuse to defer correctness, memory accounting or license checks. Independent M5 models may proceed after their dependencies are ready, but loaded structural feature surgery requires those feature codecs before full M4 acceptance can be claimed.

## M2.1: Text semantics and reusable string codecs

- Specify public behavior for plain inline/shared strings, whitespace, empty strings/runs, XML entities, Unicode, phonetic text and rich text. Probe the pinned public reference and format fixtures without broadly reading reference implementation.
- Distinguish literal `_xHHHH_` strings from protected sequences and format escapes. Preserve documented openpyxl behavior and record any necessary format-policy distinction; do not apply blanket decoding that changes literal user strings.
- Share text decoding/encoding rules across readers, sequential writers and original-package edits. Remove the current literal-escape writer rejection only after both reference interoperability and XML correctness are verified.
- Add typed rich-text runs and shared run formatting, with plain-text projection as an explicit option. Do not silently flatten a requested rich-text model.
- Keep source scalar decoding incremental; bound individual values, XML events and rich-run count/payload. Retain exact integers, booleans and error tokens without coercion.

Acceptance: both inline and shared fixtures preserve intended text and whitespace; literal/protected escapes, multiple runs, phonetics, empty values and malformed input have explicit tests. Selected public reference cases retain original assertions and licensed provenance.

## M2.2: Large shared-string storage

- Resolve the shared-string part through workbook relationships; do not assume `xl/sharedStrings.xml` or trust declared unique counts as allocation sizes.
- Parse each entry incrementally through EOF/CRC, enforcing input, entry, decoded-value and total temporary-storage limits. Index IDs without constructing a worksheet or whole-table XML buffer.
- Provide an in-memory table for measured smaller workloads and a disk-backed data/index store for larger tables. An index that grows without a budget is not a bounded-memory implementation.
- Use a byte-bounded cache including keys, decoded payloads, capacities and bookkeeping. Cache hits may borrow shared data internally, but returned owned rows remain valid after workbook/cache destruction.
- Support explicit memory/disk/cache settings and Auto selection. Use effective availability, headroom, workload samples and actual accounted bytes, with bounded spill/fallback when estimates fail.
- Keep temporary files owned by the workbook string store; handle failed parsing, cancellation, repeated readers and Drop cleanup. State whether a completed index can be reused and what CRC validation has occurred.

Acceptance: repeated-string and high-cardinality tables work under small managed RAM budgets via disk storage. Invalid IDs, missing parts, corrupt ZIP/XML, low disk allowance and cleanup failures are tested. Compare in-memory and disk policies using the same verified outputs, measuring wall/CPU, RSS, temporary bytes and cache diagnostics.

## M2.3: Read-side styles and dates

Checkpoint: indexed palette entries share ArgbLiteral, preserving mixed case and six-digit alpha normalization through source adoption and export with actual-capacity accounting (ADR 0039). The additional four bytes per palette slot are disclosed and measured; complete style mutation remains open.

Checkpoint: temporal output preserves any existing date/duration format and derives shared variants for non-date styles, using public default codes and the existing borrowed deduplication index (ADR 0038). Owned-bank temporal assignment now resolves shared style IDs before save (ADR 0040); complete style mutation remains open.

Checkpoint: typed differential overrides and table/pivot definitions/defaults are verified in ADR 0033, including all 28 region tokens and bounded deferred reference checks. Extension payloads, full style mutation/assignment semantics and worksheet rule/table graph integration remain required.

- Parse styles, number formats, fonts/fills/borders/alignment/protection, colors and themes into the shared core catalog, retaining imported style IDs and deduplicating new formats safely.
- Separate format classification for value semantics from complete style editing. A formatting index alone must not force a numeric value into a date.
- Decode both date systems, supported ISO date cells, time-only values and durations. Specify serial-zero/early-1900/fictitious-leap-day behavior, negative serials, precision and invalid inputs against public reference behavior.
- Reuse the existing date/value types and writer catalog rather than introducing a second reader style/date model. Expand those types when required semantics are currently unrepresentable.
- Account for parsed catalog memory and bound record counts and payloads. Preserve extension data separately from typed support.

Acceptance: styled numeric/text/date/time/duration fixtures round-trip through read/create/edit; tests inspect values and style attributes rather than only ZIP validity. Missing/invalid style references fail with part/cell context. Styled large-file measurements include catalog costs.

## M2.4: Formula/value completion

Checkpoint: Compatible normal/shared reads ignore unused hints and read unknown type text, while XML validation and strict group/editor checks remain intact (ADR 0035). Raw structured flag ownership and compatible array/table unused hints are additionally verified in ADR 0036. Literal/missing shared IDs now use shared canonical identities with strict numeric validation remaining optional (ADR 0041). Remaining header/cache cases stay open.

Checkpoint: optional/literal array text and literal formula reference/input ownership are verified in ADRs 0031, 0032 and 0034. Geometry is checked by explicit physical operations and strict shared-group reads; public property/save representation differences are tested. Remaining formula/cache/header and graph cases below keep this milestone open.

- Support shared formulas and follower expansion using the canonical A1 translation engine; validate master IDs/ranges and avoid dense range allocation.
- Represent array, data-table and baseline dynamic formula metadata independently of normal formula text and optional typed cache.
- Preserve absent caches, empty string caches, errors and dates distinctly. Implement normal/data-only reads without calculating formulas or fabricating results.
- Bound expression, metadata, shared-master and cache allocations; projection must avoid unnecessary decoding without corrupting later reference state.
- Define supported non-finite and precision-sensitive numeric behavior through reference tests; unsupported current cases remain open until covered.

M2 acceptance: all assigned value/string/date/formula read cases pass in streaming and explicit materialization, with high-cardinality disk-backed strings, mode restrictions, aggregate catalog accounting and representative benchmarks. M2 stays in progress until these requirements are met.

## M4.1: Loaded workbook ownership

Checkpoint: owned set/append resolves temporal style IDs before committing cells with joint prospective-cell/style accounting, shared canonical presets and reusable failure state (ADR 0040). Raw standalone models remain catalog-free; loaded graphs and general repeated styled save remain open.

Checkpoint: canonical owned-bank style import/registration, aggregate theme/style/sheet budgeting and consuming registry/sheet export are verified in ADR 0030 and benchmarks/m4-bank-styles.md. Lazy loading, repeated non-consuming styled save and original feature graphs remain open; this checkpoint does not complete M4.1.

- Load selected worksheets lazily into the existing owner-scoped Workbook bank with stable SheetId handles. New and loaded sheets must not become unrelated public model systems.
- Aggregate catalog, retained sheet models, overlays, caches and transient editing allowances; distinguish managed accounting from a hard process RSS limit.
- Preserve source ownership and allow explicit full materialization. File/path/seekable input and output lifetimes, repeated saves, close, cancellation and failed-save retry must be documented.
- Keep live owned handles safe across remove/reorder/copy; full copies include supported feature graphs rather than just scalar cells. Sharing immutable catalogs must not leak subsequent mutable edits.

Acceptance: mixed loaded/new sheet workflows, retained aliases, failed budget mutations and source release behave consistently. Python tests exercise the canonical implementation without implementing missing feature logic in Python.

## M4.2: Package graph and workbook mutations

- Centralize relationships, content types, part allocation and imported IDs; use normalized relative targets and strict/transitional namespaces.
- Support existing-file sheet create/copy/remove/rename/reorder/visibility/active selection, plus macro/template and external-link policies.
- Synchronize affected workbook declarations, relationships, catalogs and owned dependent parts. Do not remove a part that still has a valid consumer.
- Preserve opaque unknown parts/subtrees and reusable binary sources without duplicating all image bytes in RAM.
- Extend calculation-chain policy and signature guards using explicit reference ownership; an unknown potentially affected graph must fail safely until its semantics are implemented.

Acceptance: repeatable existing-file sheet mutation retains unaffected assets, names and relationships. Verify graph integrity and public feature readback; include missing/custom part names and shared consumers.

## M4.3: Structural edits and feature interactions

Checkpoint: known array/data-table replacement accepts canonical literal flags while unknown-record/attribute and shared-group guards remain intact (ADR 0037). This does not implement structured formula range transformations or the broader feature interactions below.

- Implement loaded append, row/column insert/delete, copy and move using sparse bounded transformations. Update dimensions and affected feature coordinates according to explicit reference-compatible behavior.
- Define behavior separately for formulas, merges, tables, names, validation, conditional formatting, comments, hyperlinks and drawing anchors. Do not invent automatic reference updates that openpyxl itself does not promise.
- Validate all affected ranges, budgets and references before applying an atomic edit. Avoid cloning an entire worksheet as a rollback strategy.
- Reuse unchanged compressed parts where safe; changed worksheet XML may stream to bounded spools. Specify per-operation temporary limits and aggregate output costs.

M4 acceptance: unchanged and edited XLSX/XLSM/XLTX/XLTM workflows, repeated saves, sheet and structural mutations, unknown-part preservation, complete supported feature graphs and output-failure cleanup pass. Dependencies on M5/M6 graph codecs remain visible; opaque preservation alone cannot close M4 editing gaps.

## M5: Common feature families

Checkpoint: selected rust_xlsxwriter viewport composition/serialization is integrated into canonical SheetViews/SheetView/Pane/Selection models (ADR 0042), with all baseline worksheet-view attributes, freeze/split panes, multiple views/selections, bounded header reading, creation, owned-sheet copy/budgeting and repeatable original-package editing. Pure display changes retain calculation caches/chains. Unknown view extensions, chartsheet/workbook/custom views and other M5.2 families remain open. This does not complete M5.


| Checkpoint | Core models and format work | Required acceptance |
|---|---|---|
| M5.1 | Full styles, named styles, themes/colors, row/column styles and rich text | Read/create/edit style catalogs with stable references and bounded deduplication |
| M5.2 | Dimensions, groups/outlines, hidden/merged cells, freeze panes, sheet/workbook views and properties | Sparse operations and save/reopen preserve all modeled properties |
| M5.3 | Names, hyperlinks, calculation settings, document/custom properties | Scope, external targets, metadata types and relationships remain correct |
| M5.4 | Tables/styles/structured references, filters and sort conditions | Pair serializers with readers and existing-file edit tests |
| M5.5 | Data validation and complete baseline conditional-formatting rules | Formula/range/style references survive edits; distinguish x14 support from opaque preservation |
| M5.6 | Comments, authors, dimensions/position and notes/VML | Typed access/edit plus separate preservation limits and safe shared author tables |
| M5.7 | Printing, areas/titles, margins/setup, headers/footers/breaks, workbook/sheet protection | Public interoperability and reference-compatible validation |
| M5.8 | Full formula tokenizer and translation utilities | Public token/reference cases, shared/array/data-table integration and explicit no-calculation scope |

Each family is complete only after its assigned baseline cases and relevant read/create/edit/preserve modes are verified. Reuse pinned rust_xlsxwriter serializers where suitable, but add missing readers and editing paths; a writer-only port is insufficient.

## M6: Advanced feature graphs

| Checkpoint | Scope | Required acceptance |
|---|---|---|
| M6.1 | Images, supported formats, anchors, drawings and relationships | Read/create/edit, caller-source ownership, repeated saves, bounded binary handling |
| M6.2 | All baseline chart families including applicable 3D types, chartsheets, axes/series/labels/layout and combined charts | Actual data/reference/style assertions across creation and existing-file edits |
| M6.3 | Pivots, cache definitions/records, sources, fields and baseline editable settings | Streaming large records, stable IDs, shared caches and valid round-trips |
| M6.4 | External links/caches, VBA/macro graph policies, extensions, cm/vm metadata, dynamic-array and rich-value parts | Separate typed behavior and opaque preservation; synchronize affected references |

Evaluate pinned Rust upstream code for concrete gaps, including additional projects if needed. Check license compatibility, required notices, data models and allocation behavior before porting. Record source files/symbols and semantic changes in provenance. Integrate ports into shared core/style/formula/package infrastructure and project formatting/lint conventions; do not wholesale-wrap upstream libraries or leave parallel models behind.

## M7: Stabilization and performance release gates

- Complete the behavioral matrix, regression fixtures, public docs/examples, feature flags, typed errors, MSRV/version policy and supported-platform evidence.
- Test streaming, materialized and editable modes independently. Include corrupted input, partial reads, limits, atomic failures, repeated saves and resource cleanup.
- Validate licenses, notices, inventory/provenance and packaging. Run formatting, Clippy, tests and warning-free API documentation with the pinned toolchain.
- Measure Linux and supported desktop-platform availability policies; document fallback probes. WASM-compatible core boundaries remain relevant, but shipping other language adapters is not this core release gate.
- Establish equivalent read/write/edit workloads for numeric, repeated/high-cardinality text, styled, sparse, multisheet and advanced-feature data. Separate native Rust and Python same-call comparisons.
- Pin openpyxl, calamine and rust_xlsxwriter versions; warm up and alternate repeated runs. Report verified outputs, wall/CPU, peak RSS, temporary storage and output size, with complete raw evidence.
- Faster than openpyxl is required for agreed representative equivalent workloads. Desired faster/lower-RAM native-overlap goals remain visible when unmet; do not claim universal superiority from one fixture. Current calamine read-speed gap and missing rust_xlsxwriter direct comparison are open tasks.
- Tune allocations, decoding, shared-string caches, indexes, compression and independent-sheet concurrency only after profiling. Auto should spend extra RAM where measured throughput benefits justify it; larger buffers alone are not an optimization.

Rust completion requires the full baseline matrix and all milestone acceptance gates. No reduced feature release or percentage estimate substitutes for that verification.

## Checkpoint workflow

Each coherent checkpoint includes implementation, necessary tests, source/license records, documentation, inventory status and applicable performance evidence. Follow the installed Coherent Commits skill, verify English-only content and cmostw identity, then commit and immediately push before creating the next commit. Update milestone evidence with historical results labeled as historical; do not rewrite prior measurements as new results.

When core changes affect Python tests, update the standalone adapter's exact core Git revision after publishing core. Keep openpyxl-compatible calls and unchanged selected assertions; extensions remain separately documented. Re-run relevant adapter checks without introducing a second engine or silent fallback.
