# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Before version 1.0, a minor version can change the public API.

The Python package has its own
[changelog](https://github.com/crabxl/crabxl-python/blob/main/CHANGELOG.md).

## [Unreleased]

### Changed

- Large worksheets read on several threads. Worker threads parse blocks of
  plain rows, look up shared strings and convert dates while one thread
  decompresses the part. Rows with formulas, inline strings or other content
  are read as before, so values and errors do not change.
- Saving an edited large worksheet copies its unedited rows as they are.
  Worker threads check that those rows are plain; edited rows and other
  content are rewritten as before.
- All parallel work shares one pool of threads. It uses at most the CPUs the
  process may run on, including on Windows, and starts threads only while the
  work falls behind. Set `CRABXL_MAX_THREADS` or call `set_max_threads` to use
  fewer; 1 keeps all work on the calling thread.

## [0.1.2]

### Changed

- Reading is faster on large sheets. Plain cell tags are read directly instead
  of through the general XML attribute parser.

## [0.1.1]

### Changed

- Edits no longer fail on content CrabXL does not model. As in openpyxl, that
  content is dropped or kept as written:
  - A changed signed package is saved without its signature.
  - Replacing a shared formula master writes its other cells as ordinary
    formulas.
  - A sheet with unknown content is copied from its cells, styles and
    dimensions. A removed sheet that other parts still use stays in the
    package.
  - Unknown alternatives, sheet catalog content and style extensions are
    dropped when their part is rewritten.
  - External links are removed without checking formulas, and a calculation
    chain no longer blocks edits.
- Readers ignore unknown XML attributes and elements, and read dialog and macro
  sheets like worksheets.
- Invalid input returns `ErrorKind::InvalidData` or `ErrorKind::InvalidState`.

### Added

- Moving and removing sheets of a write-only workbook.
- Changing the date epoch and ISO date storage of a loaded workbook, and the
  date epoch of a streaming reader.

## [0.1.0]

First release.

### Added

- Streaming reads of XLSX, XLSM, XLTX and XLTM files: values, shared and rich
  strings, dates, formulas with cached results, styles and a limited row or
  column range.
- Sequential writes of one or more worksheets at the same time, with a
  constant amount of memory.
- Edits of existing files that keep unchanged package parts as they are.
  Appended rows of a sheet that is not loaded are written on save without
  loading the sheet.
- Loaded workbooks: create, copy, remove, rename and move sheets; append rows;
  insert and delete rows and columns; copy and move ranges. Normal formulas
  are translated.
- Cell appearance: number formats, fonts, fills, borders, alignment,
  protection, named styles, themes and row and column styles.
- Merged cells, row and column dimensions, outline groups, hyperlinks, rich
  text, sheet views, sheet properties and print settings.
- Worksheet features: tables, auto filters and sort state, conditional
  formatting, data validation, comments, sheet protection, headers and footers
  and scenarios.
- Workbook features: defined names, print areas and titles, workbook
  protection and document properties.
- Images, shapes, charts, chartsheets, pivot tables, external links and VBA
  projects.
- Memory allowances: explicit budgets, automatic budgets from available memory,
  and `LoadOptions::with_memory_limit` to set one limit in one call. Shared
  strings can move to disk when memory is low.
- ZIP compression levels 0–9 with zlib-rs or native zlib. Worksheets of 4 MiB
  or more are compressed on up to four threads.
- The `quickstart` example.

[0.1.0]: https://github.com/crabxl/crabxl/releases/tag/0.1.0
