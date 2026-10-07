# Alpha 9–28 delivery and umya integration plan

## Status and scope

This is the active plan for the complete feature scope originally assigned to Alpha 9–28. Alpha 6–10 are published history, including verified public Rust and Python Alpha 9/10 artifacts. On 2026-10-07 the user approved consolidating the remaining work into approximately six releases to reduce repeated packaging and publication overhead. No feature scope is removed.

A11–A16 below are the current release numbers. W11–W28 are stable work-package identifiers corresponding to the former alpha targets; they are no longer individual releases. W11 implementation is underway; incomplete work is not published as a completed feature.

Rust and Python must expose each release's applicable usable functionality. Plan-only changes, refactors and disconnected ports do not advance alpha numbers. Numbers are manually selected; urgent usable fixes may insert a release and shift subsequent targets. No release dates or effort percentages are implied.

## Primary port source

Use umya-spreadsheet 3.1.0, pinned at `aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b`, as the primary source for editable models and M5/M6 feature implementations. Candidate paths below are relative to its `src/` directory and have been checked against the local pinned checkout. They identify investigation/port starting points, not audited complete behavior or already imported modules.

Existing compact worksheet ranges and native formula lexical states are selected umya adaptations. Native style-only assignment and canonical temporal export are implemented in [ADR 0083](decisions/0083-style-only-cell-assignment.md); A9 completes loaded-model integration and Python number-format exposure; complete component objects follow in A10. Existing calamine-derived parsing and rust_xlsxwriter-derived serializers stay where suitable; do not replace working fast codecs merely to standardize origin.

## Consolidated publication schedule

| Release | Included work packages | Completion gate |
| --- | --- | --- |
| A11 | W11–W13 | Named styles/themes, row/column styles, merges/dimensions/outlines, hyperlinks and editable rich text; Rust/Python integration and applicable structural behavior. |
| A12 | W14–W19 | Tables/filters, rules, comments, printing/protection, names/properties/formulas, and full M5 acceptance. |
| A13 | W20–W23 | Images/anchors, baseline chart families/chartsheets, loaded drawing/chart graph edits. |
| A14 | W24–W25 | Pivots/caches/records, external links, macro/template policies and complex metadata graphs. |
| A15 | W26 | Complete M4/M6 feature-aware integration and acceptance; close milestones only when their full matrices pass. |
| A16 | W27–W28 | Full-feature performance/RAM work, compatibility/platform acceptance and M7 release quality. |

Implement a coherent feature group through the native model, codecs and Python interfaces before adding missing behavioral checks and fixing acceptance failures together. During implementation, prefer necessary compilation checks over repeatedly running the full suite. Reuse existing high-value regression cases; do not add tests for every small helper or merge cases merely to make the test count smaller. Every publication still requires its relevant correctness, data preservation, performance and packaging gates.

## Work-package matrix

| Work package | Usable scope | umya candidate sources | Integration and acceptance focus |
| --- | --- | --- | --- |
| A9 | Style assignment and explicit temporal formats | structs/style.rs; reader/xlsx/styles.rs; writer/xlsx/styles.rs | Connect the existing native checkpoint to loaded styles and Python cell style assignment; retain explicit General on temporal values. |
| A10 | Complete cell style components | structs/font.rs, borders.rs, alignment.rs, protection.rs, stylesheet.rs | Complete shared font/fill/border/alignment/number-format/protection codecs and Python style objects. |
| W11 | Named styles, themes and row/column styles | structs/cell_style.rs, cell_styles.rs; reader/xlsx/theme.rs; writer/xlsx/theme.rs | Integrate named-style ownership, theme/color resolution, row/column formats and deterministic deduplication. |
| W12 | Merges, dimensions and outlines | structs/merge_cells.rs, rows.rs, columns.rs; reader/xlsx/worksheet.rs | Attach compact ranges to merges and sparse row/column properties; implement grouping and applicable structural interactions. |
| W13 | Hyperlinks and rich text | structs/hyperlink.rs, rich_text.rs; reader/xlsx/shared_strings.rs; writer/xlsx/worksheet_rels.rs | Reuse existing rich-run/string codecs; add typed hyperlink relationships and editable rich content with Python interfaces. |
| W14 | Tables, filters and sorting | structs/table.rs, auto_filter.rs; reader/xlsx/table.rs; writer/xlsx/table.rs | Add table identities, structured references, filters and sort settings; fill missing source coverage explicitly. |
| W15 | Validation and conditional formatting | structs/data_validation.rs, conditional_formatting.rs, conditional_formatting_rule.rs, differential_formats.rs | Share bounded range/formula/style references; support baseline rule families and applicable extension codecs. |
| W16 | Comments and VML | structs/comment.rs; reader/xlsx/comment.rs, vml_drawing.rs; writer/xlsx/comment.rs, vml_drawing_rels.rs | Integrate authors, note text, size/position and VML ownership; distinguish baseline notes from threaded-comment preservation. |
| W17 | Printing and protection | structs/page_setup.rs, page_margins.rs, header_footer.rs, workbook_protection.rs; helper/crypt/ | Extend existing print models with areas/titles, header/footer edits, printer relationships and sheet/workbook protection. |
| W18 | Names, properties and formula metadata | structs/defined_name.rs, properties.rs, cell_formula.rs; helper/formula.rs; reader/xlsx/doc_props_custom.rs | Complete defined-name scopes, core/custom properties, token tools and shared/array/data-table/dynamic formula metadata; no calculation engine. |
| W19 | M5 integration and acceptance | All A9–W18 selected ports; reader/xlsx/workbook.rs; writer/xlsx/workbook_rels.rs | Complete corresponding Python support, loaded sheet create/copy/remove for implemented graphs, structural interactions and regression profiling. Close M5 only when its full matrix passes. |
| W20 | Images and drawing anchors | structs/image.rs, anchor.rs, drawing.rs; reader/xlsx/drawing.rs; writer/xlsx/media.rs, drawing_rels.rs | Use shared anchor/relationship models and source-backed binary payloads; implement supported image creation, read, edit and copy. |
| W21 | Common charts | structs/chart.rs; structs/drawing/charts/; reader/xlsx/chart.rs; writer/xlsx/chart.rs | Implement bar, line, pie, area and scatter chart creation/read/edit with axes, series and applicable Python chart interfaces. |
| W22 | Remaining chart families and chartsheets | structs/drawing/charts/; reader/xlsx/chart.rs; writer/xlsx/chart.rs | Complete baseline chart families, combinations, layout and chartsheets; source availability does not establish coverage. |
| W23 | Loaded chart graph edits | reader/xlsx/drawing.rs, chart.rs; writer/xlsx/drawing_rels.rs, worksheet_rels.rs | Complete loaded chart copy/remove/edit and affected anchor/series interactions, shared consumers and repeatable saves. |
| W24 | Pivots, caches and records | structs/pivot_table.rs, pivot_cache_definition.rs, cache_field.rs; reader/xlsx/pivot_table.rs, pivot_cache.rs; writer/xlsx/pivot_cache.rs | Audit record coverage first; integrate bounded cache/record storage, baseline reading/preservation and editable settings. Do not promise pivot calculation. |
| W25 | External links, VBA and complex metadata | reader/xlsx/vba_project_bin.rs, workbook_rels.rs; writer/xlsx/vba_project_bin.rs, content_types.rs | Implement macro/template policies, external-link caches and metadata/cm/vm/dynamic-array/rich-value graphs. External-link and metadata source coverage is unverified; budget original work or additional pinned sources. |
| W26 | M6 and deferred M4 acceptance | All W20–W25 selected ports and shared package graph | Complete feature-aware worksheet and row/column mutations, source lifetime, opaque preservation and graph integrity. Close M4/M6 only with their complete acceptance matrices. |
| W27 | Full-feature speed and RAM tuning | CrabXL profiling; umya, calamine and rust_xlsxwriter as pinned performance references | Optimize measured read/write/edit costs across scalar, styled, text and graph workloads; retain functionality and record RAM/disk tradeoffs. |
| W28 | Compatibility and release-quality acceptance | No new bulk import; full behavioral inventory and platform matrix | Finish Rust/Python compatibility, docs, licenses, error/failure cleanup, packaging and fresh installs. Close M7 only after all remaining gates pass. |

## Integration architecture and dependencies

1. Audit each selected source at the pinned revision: symbols, dependencies, actual supported behavior, defects and unsupported variants. Record exact origin/destination, MIT notices including applicable upstream notices, behavior changes and tests in `third_party/ports.json`. Planned candidates must not be marked ported or verified.
2. Adapt useful algorithms and feature layouts into `crabxl-core` canonical models. Reuse typed coordinates/ranges, shared style/formula/string IDs, stable worksheet identities and the workbook bank. Keep umya types out of the public facade and avoid a second workbook engine or wholesale crate wrapper.
3. Put XML/ZIP codecs and package I/O in `crabxl-xlsx`. Share validation between readers, writers and original-package editors. Centralize relationships, content types, imported IDs, unknown extensions and dirty ownership. Extend existing print/view/string/style infrastructure instead of introducing parallel catalogs.
4. Integrate native models with loaded originals, repeatable borrowed saves and consuming exports. Retain source-backed unchanged parts and binary assets. Validate mutations before applying them; never clone the entire workbook as rollback. Each owning feature release must resolve its deferred M4 graph cases or leave named cases explicitly pending.
5. Add thin openpyxl-compatible Python objects/calls in the separate binding repository, pinned to the verified core commit. Preserve live handles and observable mutations. Do not implement missing spreadsheet features in Python or silently fall back to another engine.

W11 depends on A10 style identity/codecs. W12 supplies geometry and structural infrastructure used by W14–W17. W14–W18 use the existing formula foundation and must add required missing formula cases in their own gate rather than waiting for W18. W17 print names requires a minimal shared defined-name implementation before W18's full name coverage. W20 establishes drawing ownership for W21–W23. W24 may proceed only after cache/record coverage is audited; W25 complex graph work may require new original implementations. Release boundaries may change for a real dependency, with an updated plan rather than silently reduced scope.

## Resource and performance contract

Do not copy umya's buffering, ownership or clone patterns without measurement. Use compact sparse geometry, shared immutable catalogs, borrowed decoding, byte-accounted work buffers and incremental XML. Streaming modes must not materialize worksheet XML or the entire cell range. Large strings, image payloads and pivot records need explicit source-backed or disk strategies, aggregate resource accounting and temporary-file cleanup.

Performance work accompanies every relevant port, not only W27. Compare equivalent modes and supported behavior against pinned umya, calamine, rust_xlsxwriter, python-calamine and openpyxl as applicable. Faster than openpyxl is required; lower read time and RSS than calamine, and overlapping write improvements against rust_xlsxwriter, remain desired targets that must be measured rather than asserted. Report unmet targets. No universal superiority claim follows from one fixture.

## Gate for each consolidated alpha

- Record read/create/edit/preserve and streaming/materialized/sequential restrictions separately. Opaque preservation is not typed feature support. Unsupported affected operations return explicit typed errors before mutation or replacement.
- Verify feature assertions after save/reopen, repeated saves, applicable native/Python calls and relevant deferred structural interactions. Cover unknown/custom relationship targets, shared consumers and failure cleanup where applicable.
- Extend existing high-value tests where practical; retain selected upstream assertions and provenance. Avoid duplicated permutations, implementation-mirroring tests and timing thresholds.
- For relevant parser/writer/allocation changes, record representative release measurements with warmups and alternating runs: timing, peak RSS, temporary storage, output size and feature/checksum validation. Identify exact revisions and resource/compression settings.
- Pass applicable formatting/lint/MSRV/platform checks. Retain Rust 1.88 MSRV; Python builds use the latest validated stable Rust, CPython 3.11–3.15 and the existing five wheel platforms.
- Only after the usable gate passes, prepare matching manual Rust/Python alpha numbers and release workflows. Verify published crates, OIDC Python artifacts and fresh installations. Planning edits alone must not change package versions, tags or release requests.

## Milestone closure

W19 targets full M5 closure; W26 targets full M4/M6 closure; W28 targets M7 closure. Each closure requires its entire behavioral acceptance definition, including missing upstream capabilities and Python integration where promised. If cases remain unresolved, leave the milestone open and schedule an additional usable alpha. Formula evaluation, other language bindings and capabilities outside the pinned baseline are not added as hidden completion requirements.
