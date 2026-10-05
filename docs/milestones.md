# Milestone acceptance evidence

## M0: Complete architecture and baseline audit index

- Pinned openpyxl 3.1.5, calamine, and rust_xlsxwriter references and license/provenance records: third_party/ports.json and third_party/README.md.
- Public metadata catalog: 190 modules, 567 classes with inherited public members, public functions/exports, and 45 pinned release RST documents. No implementation source or bytecode was inspected to generate this catalog.
- Reproduce with `python tools/catalog_openpyxl_baseline.py --reference-checkout /workspace/openpyxl` using openpyxl 3.1.5; map and validate coverage with `python tools/map_baseline_inventory.py`.
- Every cataloged module and document has a separate staged inventory entry. Read/create/edit/preserve remain separately planned; narrow implemented checkpoints do not mark whole baseline modules verified. This completes the architecture audit index, not all semantic specifications or support.
- One runtime-independent core provides values, addresses, errors, resource limits, memory policies, and shared StyleId identity. Complete style/formula models belong to later codecs.
- ADRs 0001-0003 cover ZIP ownership, adaptive allocations, and writer port restrictions. The writer design records the upstream silent late-row write risk; no writer implementation is claimed.
- MR and work-item reviews are linked in the roadmap. Additional Rust sources remain eligible for concrete M6 gaps.

## M1: Complete raw numeric streaming acceptance

- Relationship-based package/sheet discovery; strict/transitional namespaces; sparse rows; early projection; reusable row buffers; owned bounded batches; explicit materialization and adaptive access policies.
- Integration coverage includes malformed coordinates/XML, limits, CRC after full consumption, reopening, owned output lifetimes, source transfer with into_inner, and source release on archive failure. Dropping an unfinished row stream releases the borrow, not the workbook source; it does not drain unread data or promise CRC validation.
- Recorded five-run 10k/100k/1m-row, ten-column benchmark counts/checksums, CPU/wall time, and native peak RSS are in benchmarks. Fixed-setting numeric streaming RSS remains about 1.5 MiB across these workloads; this is measured evidence, not a universal RSS guarantee.
- Materialized/Auto repeated-access evidence demonstrates the performance-memory tradeoff. Explicit budgets constrain managed retained data; dependency allocations, caller output, and allocator overhead are not a hard process RSS cap.
- Acceptance is raw finite f64/empty cells. Integer precision, strings, boolean/error/date/formula/style semantics are M2. Unsupported selected content returns errors rather than claiming support. Style index zero is not interpreted.
- Closure changes add style identity and lifecycle verification without changing the numeric parser hot path; existing release benchmark evidence remains applicable.

## M2: Complete core acceptance

The [alpha.5 audit](validation/alpha5-m2-acceptance.md) verifies every assigned
value/string/date/formula read gate, bounded high-cardinality SST storage,
read-side style catalogs, actual payload accounting and representative release
measurements. Exact style integers, explicit theme/font catalogs, owned formula
annotation references, native host probes and concurrency-aware Auto finish the
remaining checkpoints. Linux/Windows/macOS Rust 1.88 default/native-zlib and
latest quality checks pass in CI run 37293401537. Full baseline modules and
M4–M7/adapter capabilities remain independently staged.

Historical initial M2 checkpoints below record what was known at their time;
current status and limitations are those of the linked acceptance audit.

The first verified checkpoint adds typed booleans through the shared streaming path without increasing CellValue storage. Integer precision, errors, shared/inline/rich strings, styles/date systems, formula caches, and disk-backed high-cardinality strings remain required. Individual checkpoints update the verified feature matrix. M2 completion requires mixed-value correctness, large-string resource/cleanup evidence, and representative release benchmarks; M1 numeric results do not establish it.

The boolean checkpoint has 42 passing tests (including the facade doc test), lint/documentation checks, and [release count/RSS/regression evidence](../benchmarks/m2-boolean.md). The measured numeric slowdown remains an optimization item; M2 is not accepted as complete.

The next M2 scalar checkpoint preserves i64 and arbitrary decimal integers, integer/float lexical distinctions, plain inline/value text, and error literals through the shared model. Its 46 tests and release mixed-value evidence are in [m2-scalars.md](../benchmarks/m2-scalars.md). Heap payloads count toward every retained-data budget; heterogeneous-text Auto fallback is tested. Shared strings, rich text, dates/styles/formulas, and escape-aware text codecs remain required before M2 completion.

## M3: Complete sequential writer acceptance

- Selected rust_xlsxwriter spooling, packaging, cell/formula XML and initial style serializers are integrated with the shared core. No upstream workbook/model wrapper is exposed. Date conversion adapts the pinned chrono epoch/leapday path and uses the foundational chrono crate.
- Creation covers exact scalars, plain inline text, date/time/duration values in both epochs, normal formulas and optional typed caches, and basic font/solid-fill/four-border/alignment/number-format/protection formats. Style IDs are shared and deduplicated, with bounded metadata/counts. No calculated or fabricated numeric cache is supplied.
- The reader shares the normal formula/cache model, including data-only selection; complete date/style reading and non-normal formula metadata remain M2. Scalar/formula Rust round-trips and cross-tool openpyxl 3.1.5 normal/data-only date/style readback are verified. This is creation interoperability, not existing-file preservation or the complete M2 codec.
- 61 deterministic tests (including the facade doc test) cover values, early date boundaries/epoch conversions, sparse/multiple/empty sheets, projections and payload budgets, invalid formulas/style references, sequential restrictions, temporary limits, creation/output/cleanup failures, repeated abort and abandonment. Formatting, Clippy and warning-free API documentation pass.
- [Release evidence](../benchmarks/m3-writer.md) records CPU/wall/RSS, exact and sampled temporary storage, output sizes, checksums/types/style attributes and internal relationships/collection counts. Numeric output is measured through one million rows; mixed/feature workloads through 100,000 rows in both epochs. RSS excludes OS file cache/tmpfs costs; bounded RSS is not zero disk cost.
- M3 acceptance in the roadmap is satisfied. Shared/rich strings, escape-aware text, full read-side dates/styles, advanced styles/formulas, editable/preserved files and non-seekable sinks remain explicitly staged in M2/M4/M5/M6/M7.

## M4: In progress, sparse/preservation checkpoint

- Sparse core worksheets support budgeted random access, append, finite row/column insertion/deletion and overlapping range copy/move with atomic validation. Borrowed output reuses the sequential writer. Formula/reference translation and existing-file structural surgery are not claimed.
- Lazy editor inventories original parts and keeps only pending cell values. Unchanged payloads, images, VBA, templates, relationships and unknown parts survive repeat saves. Existing-cell edits retain styles/attributes and unrelated worksheet content; old formula caches are invalidated globally and recalculation requested.
- 78 deterministic tests pass with formatting, Clippy and warning-free API documentation. Failure coverage includes input/patch/work limits, unsupported targets, CRC validation, output failure/retry, target protection and temporary cleanup. Public openpyxl fixtures verify dates/styles/comments/hyperlinks/merges/dimensions/validation/images/properties remain intact.
- [Release evidence](../benchmarks/m4-editor.md) covers three numeric scales, CPU/wall/RSS/output/temp space/checksums, public interoperability, sparse model costs and reader regression. The measured reader slowdown remains an optimization item.
- M4 is not complete: workbook sheet mutation, inserting missing cells and preserving existing feature/reference graphs during structural edits remain required. Preservation is recorded separately from typed feature access.

The next M4 checkpoint adds explicit `upsert_value`, with ordered sparse insertion into existing/empty rows, strict namespaces, preserved inferred positions, dimension growth, row-extension ordering and merged-cell guards. All 80 Rust tests pass. [Insertion evidence](../benchmarks/m4-insertion.md) covers bounded memory, checksums, public readback and cleanup. Existing-file structural edits and workbook sheet mutations remain required.

## Optional Python adapter checkpoint (parallel to M4-M7)

The user authorized an early Python binding for shared tests and easy migration. The separate Maturin/PyO3 package exposes openpyxl-compatible call names and live Cell views over the independent Rust model/editor. Core remains Python-independent. Native long operations release the GIL; budgets default to Rust Auto with explicit operation caps. All 81 Rust tests pass, and the selected Python suite has 55 passing cases with one strict expected failure for M5 formula translation. This is partial adapter coverage, not completed M4-M7 or the full openpyxl suite. Provenance verifies 37 original release test bodies. [Direct same-call API benchmarks](../benchmarks/python-adapter.md) include interpreter/conversion costs and temp storage.

## M5: In progress, A1 translation checkpoint

Selected pinned calamine reference/offset algorithms are refactored into a byte-bounded core scanner with quoted sheet/string and structured-reference context guards, checked arithmetic and baseline formula-axis semantics. Sparse translated moves prevalidate every formula and staging allowance before mutation, retaining styles and discarding translated caches. Python exposes compatible Translator helper/call names and move_range(translate=True). All 85 Rust tests and 178 Python cases pass; provenance verifies 46 original worksheet/translator methods. The prior strict expected failure now passes normally. Full tokenizer/formula metadata and all remaining common feature families stay staged. [Release evidence](../benchmarks/m5-formula.md) covers same-call translation and shared-format writer regression.

## M4: Owned workbook checkpoint

Core Workbook adds stable owner-scoped sheet IDs, aggregate managed bytes/cell/sheet limits, guarded sheet mutations, independent scalar/formula copies, reorder/rename/remove, epoch and active selection. Borrowed writer export shares the existing encoders. Python active selection/load/save parity passes. All 90 Rust tests and 180 Python cases pass; formatting, Clippy, warning-free documentation and provenance checks pass. [Release evidence](../benchmarks/m4-workbook.md) records explicit full-copy costs and writer regressions. Python models still use per-model limits; aggregate-bank adapter migration, source structural/feature graphs and complete catalogs remain required. M4 is still in progress.

## M4: Python aggregate bank checkpoint

Registered new Python Workbook sheets now borrow the canonical Rust bank through stable owned handles. Compatible copy_worksheet/move_sheet/index/deletion preserve Python list and live-alias behavior; removed retained sheets transfer into standalone caller-owned models. Total managed bytes/cells and structural work are enforced across registered sheets, with atomic failure and freed-space reuse. Loaded models still use separate per-model/overlay allowances. All 90 Rust tests and 218 Python cases pass, including 63 original pinned worksheet/translator/workbook bodies with unchanged assertions and a concurrent native-copy ownership regression. [Same-call release evidence](../benchmarks/m4-python-bank.md) beats openpyxl on the measured copy/create workload, but records an approximately 5.1% one-sheet creation regression versus earlier adapter handles and higher temporary storage. Existing feature-graph surgery and full catalogs remain required; M4 is unfinished.

## M4: Derived calculation-chain policy checkpoint

Default edited saves discard conventional override-based calculation-chain parts and synchronized content-type/workbook relationship references, invalidate old worksheet caches and request full recalculation. Explicit RejectEdits retains the earlier policy. Unchanged and reverted output preserves original chain payloads. Bounded incoming-edge checks guard unknown consumers, outgoing extensions, markup alternatives and missing types; signatures still reject edits. All 94 Rust tests and 220 Python cases pass with formatting, Clippy and warning-free documentation. [Release evidence](../benchmarks/m4-chain.md) verifies formula/numeric readback, resource behavior and an ordinary edit regression. M4 existing-file structural/feature editing remains incomplete.

## M2: Plain shared-string storage checkpoint (in progress)

Plain shared strings now resolve through workbook relationships and use incremental SST parsing with actual-entry limits instead of trusting uniqueCount. Forced RAM, forced disk and availability/budget-driven Auto placement share one table implementation. Auto spills actual over-budget payloads and the complete fixed-width index to owned temporary files. A byte-bounded direct-slot cache retains decoded values; owned rows survive cache/workbook destruction. Configuration and diagnostics expose placement, policy/retained budgets, managed bytes, temporary bytes and cache behavior. Reconfiguration releases prior storage; failed preparation leaves no installed partial table.

XML character decoding is shared with inline/scalar readers. Public openpyxl 3.1.5 probes verify protected shared literals versus unchanged inline spelling; inline writers and original-package edits accept those literal patterns. At this historical checkpoint rich/phonetic shared entries retained IDs and failed explicitly when selected; the subsequent typed checkpoint below adds those codecs. SST parsing consumes EOF/CRC before table installation. Tests cover disk/RAM agreement, ownership, repeats, Auto availability/spill, zero/bounded caches, strict namespaces, huge advertised counts, invalid IDs, resource failures and cleanup. All 101 Rust tests pass with formatting, Clippy and warning-free docs.

[Shared-string release evidence](../benchmarks/m2-shared-strings.md) compares two text cardinalities and sizes against pinned openpyxl read-only and calamine public Range. M2 remains incomplete: full read-side styles/dates, catalog integration and non-normal formula metadata are required. Catalogs and caller rows remain separate managed allowances, not a hard aggregate RSS ceiling.

## M2: Typed rich-text checkpoint (in progress)

Shared core models retain runs, optional font overrides, color identities/tints, empty styled runs and separate pronunciation metadata without redundant flattened strings. Explicit rich reading, inline creation and ordinary literal replacement share bounded codecs. Default plain projection avoids format allocations. Shared strings use the same RAM/Disk/Auto placement and complete disk index; first typed access upgrades a previously plain table by rebuilding it. Unknown extensions reject typed access; imported phonetic font assignment remains guarded pending catalogs.

Generated tests cover metadata upgrades, marker boundaries, cache/result ownership, invalid properties/characters, atomic cell limits, typed creation and repeated edits. Font/theme catalogs, full date decoding, non-normal formulas and aggregate loaded-workbook accounting remain required before M2 completion. Python typed rich compatibility is separately staged. All 110 Rust tests pass, with formatting, Clippy and warning-free API docs. [Release measurements and interoperability](../benchmarks/m2-rich-text.md) verify complete rich runs through the pinned public reference. See [ADR 0010](decisions/0010-rich-text-projections.md).

## M2 style catalog and numeric-date checkpoint

Shared optional font/color, full pattern/gradient geometry, nine border positions, alignment/protection and ID-based imported catalogs are now available. Numeric styled values and formula caches are interpreted in both epochs, with baseline clock/duration distinctions and an explicit raw-serial extension. Catalogs are prepared lazily with actual capacity/count accounting; styles are not cloned per cell. Complete creation attributes pass public openpyxl readback. [Measurement and limits](../benchmarks/m2-styles-dates.md) cover mixed date/cache streaming and numeric regression.

M2 remains in progress: theme resolution, ISO dates/durations, complete calendar construction precision, shared/array/data-table formula metadata, aggregate catalog budgets and loaded model integration are not complete. Full style editing, differential/table styles and extension export remain staged. The new catalog is shared core data; safe imported mutation and writer table normalization still require integration.

## M2 literal date precision checkpoint

Literal calendar datetimes, clock times and elapsed durations now preserve microseconds in the canonical core. Imported serials remain distinct and convert with loaded baseline millisecond rules. Early ambiguous Windows dates retain their original Gregorian day for literal epoch conversion. [Creation and mixed-read evidence](../benchmarks/m2-date-literals.md) verifies both epochs, values/types and process performance. Date-only/ISO codecs, safe loaded assignments and complete aggregate accounting remain staged; M2 is not complete.

## M2 ISO and date-only checkpoint

Date-only literals and shared ISO parsing/formatting now join the canonical date model. Type `d` cells and caches work in streaming/materialized reads; opt-in ISO creation retains early Gregorian dates in both workbook epochs, while elapsed durations stay numeric. All 42 public ISO utility observations match native output. Tests verify prefix semantics, fraction truncation, contextual malformed errors, limits, retry and cleanup. [Creation/read measurements](../benchmarks/m2-iso-dates.md) include exact native logical temporary XML and sampled disk peaks. Theme/catalog integration, advanced formulas and aggregate accounting remain required; M2 is still in progress.
