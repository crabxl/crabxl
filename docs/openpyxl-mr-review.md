# Pending openpyxl merge requests: fixes and porting risks

Reviewed on 2026-10-04. The Heptapod API listed **20 open MRs**. All titles/descriptions were reviewed; 15 received selected diff/test/context review, while five were triaged from descriptions only. This was the user's explicit exception to the initial documentation-only architecture review, not a broad implementation audit.

Titles, descriptions, branch names, HEADs, and reviewed diff refs are preserved in the [API snapshot](research/openpyxl-open-mrs-2026-10-04.json). Recheck status and pin source revisions before using a fix.

**No MR was merged locally.** Changes target 3.1, 3.2, default, and 2.6 branches; some drafts are incomplete and some introduce ownership, memory, or preservation risks. API mergeability is not proof of correctness. The notes API returned HTTP 401, so maintainer discussions were not reviewed. Full MR test suites were not rerun.

## Priority findings

### Writer lifecycle and temporary-file cleanup: !472

[MR !472](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/472) describes abandoned write-only streaming generators and temporary files, including GC-time lxml errors. It adds context-manager/closed state and mode-specific cleanup.

For M1/M3, distinguish finish (produce a valid ZIP and report errors), abort/close (release without implicit save), and Drop (last-resort cleanup). Editable repeat saves and one-shot streaming output remain separate capabilities. Test empty/partial streams, explicit/repeated close, save then close, I/O failure, and multiple sheets. Attempt cleanup of all owned resources even if one fails; the MR's sequential cleanup plus finally-closed state does not prove that property. This patch was not applied or tested locally.

### Empty text versus whitespace: !470 / !466 / !467

[!470](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/470) and [!466](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/466) address whitespace-only text without xml:space="preserve". [!467](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/467) omits empty ordinary rich-text runs.

The !470 diff matches literal tag "t" and does not demonstrate namespaced-tag coverage; !466 changes general XML whitespace behavior; !467 changes the plain-string branch, not every empty styled TextBlock. Do not treat them as interchangeable complete fixes.

M2/M3 should use a shared text encoder for inline/shared/rich text. Preserve meaningful leading/trailing/whitespace-only content; distinguish empty strings, absent values, and whitespace. Test tabs/newlines, styled spaces, empty styled blocks, and namespace variants. The 3.1.5 probe confirms an unmarked whitespace-only rich-text run and an empty text element; Excel display/repair behavior was not tested.

### Missing formula cache: !473

[MR !473](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/473) omits empty `<v/>` when no cached formula result exists, modifying both writer backends and normal/array/data-table tests.

For M2/M3, store formula and typed cached result separately. Missing cache differs from numeric zero, false, or an empty string result; generic falsy checks must not discard valid caches. Define data-only behavior without a cache. The probe confirms an empty v element in 3.1.5, but the author's Excel-repair claim was not independently verified through Excel or schema validation. Check format rules and cached-string representation before adopting a universal omission rule. Author-reported suite results were not rerun.

### Repeat image saves versus ownership and RAM: !458

[MR !458](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/458) addresses a closed stream when saving a loaded image workbook twice. The description no longer fully matches its current diff: the constructor eagerly caches image bytes and closes the image, rather than simply preserving a caller-supplied image's stream.

Additional diff concern: an image without a format can skip both encoding branches, leaving empty bytes, while path generation asserts a format. This needs verification before merging. M4/M6 should use repeatable image sources/original parts or disk-backed storage, avoid retaining duplicate bytes for every image, and specify caller-resource ownership. Test new/loaded images, repeated saves, borrowed streams, large-image RSS, and formatless conversion. The 3.1.5 public-API probe reproduces `ValueError: I/O operation on closed file.` on the second save of a loaded image workbook.

### Push projection before expensive decoding: !454

[MR !454](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/454) proposes max_col parsing. Its description notes remaining XML traversal cost and a timing-dependent test unsuitable for direct merge.

The diff filters on `self.col_counter <= max_col` before calling parse_cell, but necessary base-revision context shows parse_cell updates the counter and each row starts at zero. A/B/C with max_col=2 can still decode C; sparse A/XFD can decode XFD. This is not a filter on the current cell coordinate. Outer layers may filter the returned cells, so the finding is unnecessary decoding risk, not a claim that all public calls return incorrect values.

M1 should resolve the current coordinate before costly conversion/allocation. Unselected XML events and ZIP bytes may still require scanning. Test wide sparse sheets, first-column projection, missing coordinates, boundary cases, and selected shared-formula dependencies. Use deterministic decode counters/results for correctness and separate benchmarks for time.

### Extension and metadata preservation: !443 / !446 / !445 / !439

- [!443](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/443): generic extension trees and namespace maps are useful ideas, but introduce recursive resident models and XML-backend/dependency changes. Selected changes do not demonstrate a complete worksheet writer extension path; preservation is not verified end to end.
- [!446](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/446): the title covers validation/conditional formatting, but selected registry changes list only DataValidationX14Ext. Unknown extensions are warned about and discarded. This is not complete conditional formatting or generic preservation.
- [!445](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/445): namespace argument propagation; some overrides accept the argument without forwarding it in the changed code. Test actual encoded namespaces, not only signatures.
- [!439](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/439): metadata.xml and cm/vm references. The description warns about dynamic-array corruption from namespace/extension loss; changed documentation only claims XLMDX and discards XLDAPR/XLRICHVALUE references. This is important but not a complete repair.

M4 needs opaque extension/namespace/prefix-context preservation; M5/M6 add typed x14/metadata models. Preserve unknown subtrees via bounded or disk-backed storage rather than retaining all extension DOMs or silently discarding them. Keep cell attributes, metadata indices, dynamic-array/rich-value parts, GUIDs, and linked IDs consistent. Copying metadata.xml while dropping cm/vm still risks corruption. Do not accumulate a whole-sheet cell-to-metadata map in streaming mode.

Test unchanged/unrelated-edit extension round-trips, x14 linked rules, dynamic arrays and rich values, and metadata reference changes during structural edits. A synthetic 3.1.5 probe confirms unknown worksheet extensions are dropped with a warning. Dynamic-array behavior was not tested locally.

### External relationship identity: !455

[MR !455](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/455) replaces a single file link with multiple relationships, then deduplicates by Target. Its diff does not show remapping every consumer r:id; relationships differing in ID/Type/TargetMode may not be equivalent.

M4/M6 should use a full relationship graph, preserve identity, and define equivalence before deduplication. Test multiple links, equal targets with distinct identities/types, relative targets, and unrelated edits. This is a review risk, not a locally verified patched failure.

### Empty validations, whole-axis ranges, formula operators: !465 / !469 / !345

- [!465](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/465): after filtering rules without sqref, omit an empty validation container. M3/M5 should filter without mutating public state and write counts matching valid rules. The 3.1.5 probe confirms count=0 output; Excel repair was not tested.
- [!469](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/469): whole-row/column print areas become full Excel limits, but the patch also changes general coordinate parsing/limits. M4/M5 should represent WholeRows/WholeColumns/Rect without dense cell allocation; a local print-area fix must not silently change every coordinate utility.
- [!345](https://foss.heptapod.net/openpyxl/openpyxl/-/merge_requests/345): `OFFSET(...):C3` uses a range operator rather than only literal A1:A3 ranges. Its target is still 2.6; do not merge directly into a modern baseline. M5 needs named ranges, quoted sheet refs, whole-axis ranges, generated references, precedence, and round-trip tests.

## All 20 requests

Titles below are abbreviated. Selected-diff review does not mean full line-by-line approval or validated patches.

| MR | Topic | Review and disposition |
|---|---|---|
| !473 | Formula empty cached value | Diff; typed-cache rules and evidence limits |
| !472 | Write-only close/context manager | Diff; prioritize resource lifecycle |
| !470 | Text whitespace | Diff; group with !466 |
| !469 | Draft print area | Diff; whole-axis types and parser side effects |
| !468 | Column autofit | Description; approximate new feature, defer |
| !467 | Empty rich-text runs | Diff; distinguish empty text and spaces |
| !466 | Whitespace-only xml:space | Diff; overlaps !470, do not merge twice |
| !465 | Empty validation sequence | Diff; valid counts and omitted empty container |
| !458 | Repeat image save | Diff; reproduced baseline bug, patch memory/ownership concerns |
| !455 | Draft multiple external links | Diff; relationship identity/remapping |
| !454 | Draft max_col parser | Diff/context; stale-counter risk and scanning cost |
| !451 | Draft header/footer images | Description; later drawing/printing extension |
| !446 | Draft x14 extensions | Diff; distinguish typed support and preservation |
| !445 | Sequence namespaces | Diff; namespace-context rules |
| !444 | Table documentation examples | Description; documentation, not urgent code fix |
| !443 | Worksheet extension preservation | Diff; end-to-end output not verified |
| !439 | metadata.xml | Diff; dynamic-array/rich-value loss risks |
| !416 | Draft noncontiguous deletion | Description; potential bulk-edit extension |
| !414 | Draft worksheet search | Description; new API/type/coverage unresolved, defer |
| !345 | Formula colon operator | Diff; useful rules, obsolete target branch |

## Local observations

The probe uses installed openpyxl **3.1.5 public APIs** to create/read small files and inspects generated ZIP/XML. It does not apply MRs or modify the reference checkout. Results are baseline observations, not post-fix test reports.

With openpyxl==3.1.5 and Pillow installed, run `python tools/probe_openpyxl_mrs.py`. This emits observations rather than a golden test requiring future versions to keep defects. Saved output is in [probe results](research/openpyxl-3.1.5-probe-results.json).

Observed: empty formula v, unmarked whitespace run, empty text run, count=0 validation container, dropped unknown extension, and repeat image-save failure. No Excel GUI/schema validation or MR full-suite run was performed. Carry problems, format rules, and regression cases into Rust instead of copying incomplete patches and their defects.
