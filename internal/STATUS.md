# Status

Last update: 2026-10-24.

Replace this file when the status changes. Do not append a history log.

## Current release

- Latest release: `0.1.0-alpha.11` (crates.io and Git tag).
- The Python binding has the same release number, `0.1.0a11`, but its
  release numbers are independent.
- No release is in preparation.

## What works

- Streaming read of rows with bounded memory.
- Sequential creation of workbooks.
- Edits of existing files. Parts that are not changed are copied without
  change.
- Values: numbers, exact integers, text, rich text, booleans, errors, dates,
  times, durations and formulas with cached results. CrabXL does not calculate
  formulas.
- Cell styles, named styles, themes and row and column styles.
- Merged cells, row and column dimensions and outline groups.
- Hyperlinks, worksheet views and print settings.
- Sheet operations on a loaded workbook: create, copy, remove, rename, move,
  and insert or delete rows and columns.
- Memory limits, shared-string storage in memory or on disk, and ZIP
  compression levels 0–9.

Unsupported operations return an `Unsupported` error. They do not drop data.

## Next tasks

### Documentation (to do)

The documentation site is in [crabxl-docs](https://github.com/crabxl/crabxl-docs).
It publishes `docs/` from this repository. Follow the content specification in
crabxl-docs.

1. Write `docs/` user pages:
   - Installation and a quick start.
   - How to select an API: `WorkbookReader`, `WorkbookWriter`,
     `WorkbookEditor` or `LoadedWorkbook`.
   - Reading, writing and editing.
   - Styles, dates and formulas.
   - Memory limits and large files.
   - Errors.
   - Supported features. Generate this table from `internal/features.json`;
     do not write it by hand.
2. Add user examples to `crates/crabxl/examples`: large-file streaming and
   edit of an existing file. `quickstart.rs` is complete.
3. Add a crate-level introduction to the docs.rs page in
   `crates/crabxl/src/lib.rs`.
4. Make the README shorter. Link it to the documentation site after the site
   is published.
5. Rewrite `internal/roadmap.md` in plain language. Use the writing rules in
   [AGENTS.md](../AGENTS.md). Remove completed items.
6. Mark the status lines in `internal/validation/` as historical. Some lines
   say "not a published release" for releases that are now published.

### Features (next release)

These feature groups are planned for the next release. The full scope and
order are in [alpha-9-28-plan.md](alpha-9-28-plan.md).

- Tables, auto filters and sorting.
- Data validation and conditional formatting.
- Comments.
- Print areas, print titles, headers and footers, and sheet and workbook
  protection.
- Defined names, document properties and complete formula metadata.

### Features (later releases)

- Images and charts.
- Pivot tables, external links, macros and templates.
- Complete integration of structural edits with all features.
- Performance and memory work on the complete feature set.

## Known problems

- Most programs in `crates/crabxl-bench/examples` are benchmark drivers. They
  are not usage examples.
- `internal/features.json` is the capability inventory. It is not readable as
  user documentation.

## Terms in older records

Older records use these codes:

| Code | Meaning |
|---|---|
| M0–M7 | Milestones: M0 architecture and inventory, M1 numeric streaming, M2 values and large strings, M3 sequential writer, M4 editable model and file preservation, M5 common workbook and worksheet features, M6 drawings, charts, pivots and complex parts, M7 crate stabilization. M0–M3 are complete. |
| A6, A11, … | Alpha release numbers, for example A11 is `0.1.0-alpha.11`. |
| W11–W28 | Work packages in [alpha-9-28-plan.md](alpha-9-28-plan.md). They were release targets before the plan was consolidated. |
| ADR | Architecture Decision Record in [decisions/](decisions/). |
