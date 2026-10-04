# Upstream work items, board, and milestone review

Reviewed on 2026-10-04 alongside the [pending MR review](openpyxl-mr-review.md). The public issue API returned **379 open issues** across four pages. Metadata and titles were screened; 40 relevant issues were selected for risk review using descriptions or available excerpts; #2209 and #2322 have no description. This is a triage, not a reproduction or implementation audit. Old open reports may already be fixed; verify against the pinned 3.1.5 release before treating them as current defects. Attachments and maintainer discussions were not inspected, and no upstream changes were merged.

The [snapshot](research/openpyxl-work-items-2026-10-04.json) records issue URLs, update dates, ASCII-escaped titles, description hashes, and selected review flags. Raw upstream bodies remain outside the repository. These records are evidence of the review, not imported upstream code.

## Planning views

- [Work items](https://foss.heptapod.net/openpyxl/openpyxl/-/work_items): 379 open issues; a large backlog rather than a list of confirmed release blockers.
- [Boards](https://foss.heptapod.net/openpyxl/openpyxl/-/boards): public GraphQL exposes one board, **Development**, with **Open** and **Closed** lists. No additional workflow stages were exposed. Board membership/card order was not enumerated; this board does not establish implementation progress.
- [Milestones](https://foss.heptapod.net/openpyxl/openpyxl/-/milestones): one active milestone, **3.2**, with two open issues, zero completed issues, no merge requests, and no due date. Public HTML supplies milestone counts; REST board/milestone endpoints returned HTTP 401, so public HTML/GraphQL were used instead.

Milestone [#1999](https://foss.heptapod.net/openpyxl/openpyxl/-/work_items/1999) proposes smaller stored cells, avoiding unnecessary style allocations, cached type information, and worksheet-level hyperlinks/comments. This supports compact cells, shared typed style IDs, and sparse auxiliary collections. Its suggestion to disable external links by default must not become silent data loss: preservation remains a separate explicit capability.

Milestone [#2209](https://foss.heptapod.net/openpyxl/openpyxl/-/work_items/2209) requests range-keyed hyperlinks with worksheet and cell access, referencing #1502 for a sample. Store hyperlink ranges separately rather than duplicating a target per cell. The linked sample was not reviewed; range lookup/edit/serialization tests remain required in M4/M5.

## Required regression coverage

Numbers below link to the corresponding `/work_items/<number>` under the project URL. All findings in this table are reported risks, not independently confirmed defects in crabxl.

| Reports | Risk and required behavior | Stage |
|---|---|---|
| #1999, #2281 | Compact shared cell metadata; parse each pivot cache once through a bounded or disk-backed cache, not once per sheet | M2, M6 |
| #2233 | Huge merged rectangles must remain interval/range metadata; avoid allocating or visiting every covered cell merely to load a sheet | M4, M5 |
| #745 | Historical write-only finalization buffered whole worksheet XML. Stream temporary files into ZIP and measure finalization RSS and cleanup | M3 |
| #2240, #1195, #1243 | Read-only dimensions, empty rows, and visibility may differ from editable reads. Differential tests; preserve sparse core representation and expose explicit dense iteration where required | M1, M2, M4 |
| #577, #2278 | Strict namespaces and malformed input must produce correct results or typed contextual errors; no panics. Existing M1 tests cover strict namespaces and malformed input, not every uploaded crash fixture | M1 onward |
| #2325 | Missing formula cache differs from zero/false/empty text. Cross-reference MR !473; Excel repair claim remains unverified here | M2, M3 |
| #2298, #2243, #2235, #726 | Preserve rich-text spaces/newlines and literal OOXML escape-looking text. Share text codecs; keep namespace and escape handling explicit | M2, M3 |
| #2223 | Do not silently truncate overlong strings; return a contextual limit/validation error or use an explicitly chosen compatibility policy | M2, M3 |
| #2326 | Worksheet text whitespace rules must not be blindly applied to core properties; validate each part against its own schema | M3, M5 |
| #2299 | Empty data-validation collections can require omitting the parent element. Respect schema-specific minimum children rather than a global empty-element rule; see MR !465 | M3, M5 |
| #2289 | Reused write-only cells must reset comments as well as style/hyperlinks; verify metadata does not leak into subsequent cells | M3, M5 |
| #2323, #2237, #2322 | Range-based column dimensions, phonetic attributes, and font pitch/family validators need spec-aware models without dense expansion | M2, M5 |
| #2324, #2302 | Separate original borders from derived merged-edge appearance; validate overlapping merges before writing corrupt files | M4, M5 |
| #2209, #2320, #1273 | Hyperlink ranges and structural edits require coordinated references. Document supported reference updates; baseline insert/delete does not guarantee repairing every dependency | M4, M5 |
| #2309 | Tokenization must distinguish spill references from error tokens; preserve formulas even when translation support is incomplete | M2, M5 |
| #2306, #2304, #1270, #1244 | Test custom-view IDs, conditional-format range construction, case-insensitive name semantics, and worksheet-copy print margins. Historical reports need release-specific probes | M4, M5 |
| #2287, #2232 | External names, local/global scope, and remote URLs must survive edits without invalid indices or conversion into local paths | M4, M6 |
| #2241, #2250 | Preserve embedded objects and connection parts with their relationships; preservation does not imply readable/editable models | M4, M6 |
| #2279 | Repeat image saves must retain repeatable sources without closing caller streams or duplicating every image in RAM; see MR !458 and existing public-API reproduction | M4, M6 |
| #2253 | In-cell images can become errors after round-trip. This is a preservation gap beyond ordinary drawing images and must be represented explicitly | M4, M6 |
| #2254, #2228, #2229 | Pivot parts, chart data tables, and application properties affect Excel fidelity. #2228 concerns a chart data table, not a worksheet formula data table | M6 |

## Effect on the Rust architecture

Keep values compact, auxiliary metadata sparse, and styles shared. Smart Auto allocation may spend more memory when reuse benefits, but it must not multiply a pivot cache by sheet count or expand a merged rectangle. Preserve unknown parts and relationship graphs before rewriting existing files. Keep lossless read/preserve behavior independent of editable support, and expose unsupported mutations as errors.

These findings refine regression priorities without reducing the public-feature baseline or claiming M2-M6 support early. Implement focused fixtures when the affected codec/model is introduced; an upstream issue title alone is insufficient justification to merge a patch.
