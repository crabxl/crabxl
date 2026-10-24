# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Before version 1.0, any release can change the public API.

The Python package has its own
[changelog](https://github.com/crabxl/crabxl-python/blob/main/CHANGELOG.md).

## [Unreleased]

### Changed

- Benchmark drivers moved from `crates/crabxl/examples` to the unpublished
  `crabxl-bench` crate. The new `quickstart` example shows the main APIs.

## [0.1.0-alpha.11] - 2026-10-21

### Added

- Named styles, themes and row and column styles.
- Merged cells that you can edit, including bulk replacement.
- Row and column dimensions and outline groups.
- Hyperlinks that you can read, create and edit.
- Rich text values that you can edit.

### Changed

- Row and column insertion and deletion update merged cells and hyperlinks.

## [0.1.0-alpha.10] - 2026-10-07

### Added

- Complete cell appearance: font, pattern and gradient fill, border, alignment
  and protection, for loaded workbooks and sequential writers.

## [0.1.0-alpha.9] - 2026-10-07

### Added

- Change the style of a cell without a change to its value.
- Number formats for dates and times on export.

### Changed

- Repeated format edits on a loaded workbook do not scan all cells again.

## [0.1.0-alpha.8] - 2026-10-06

### Changed

- Faster reading of numbers and shared strings.
- Lower memory use for large loaded models. A 64 MiB allowance now holds one
  million numeric cells.
- Inserting rows in descending order uses approximately 68% less memory.

### Removed

- Default limits on the number of cells and sheets. Explicit limits are still
  available.

## [0.1.0-alpha.7] - 2026-10-06

### Added

- Loaded workbooks: create, copy, remove, rename and move sheets.
- Sheet visibility and active sheet selection.
- Append rows, remove cells, insert and delete rows and columns, and copy or
  move cell ranges. Normal formulas are translated.

### Changed

- Operations that affect unsupported features return an error before any
  change is made.

## [0.1.0-alpha.6] - 2026-10-05

### Fixed

- Valid ZIP and ZIP64 files made by streaming tools no longer cause a false
  size-limit error.

### Removed

- Default size limits on archives and worksheets. Explicit limits are still
  available.

## [0.1.0-alpha.5] - 2026-10-05

### Added

- Memory probes for Windows and macOS hosts.
- Theme palettes and font schemes.
- Automatic memory allowances that account for concurrent operations.
- Shared strings can move to disk when memory is low.

## [0.1.0-alpha.4] - 2026-10-05

### Added

- ZIP compression levels 0–9 in `WriteOptions` and `SaveOptions`. The default
  is level 6.
- The `deflate-zlib` feature selects native zlib. The default backend is
  zlib-rs.

## [0.1.0-alpha.3] - 2026-10-05

### Changed

- Edits of existing files save approximately five times faster.

## [0.1.0-alpha.2] - 2026-10-05

### Added

- Write more than one worksheet at the same time.
- Worksheet dimensions and reads of a limited row range.

### Changed

- The minimum supported Rust version is now 1.88.

## [0.1.0-alpha.1] - 2026-10-05

### Added

- First release: streaming XLSX reads, values, shared and rich strings, dates,
  style catalogs, formulas with cached results, sequential creation, cell edits
  of existing files, worksheet views and print settings.

[Unreleased]: https://github.com/crabxl/crabxl/compare/0.1.0-alpha.11...HEAD
[0.1.0-alpha.11]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.11
[0.1.0-alpha.10]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.10
[0.1.0-alpha.9]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.9
[0.1.0-alpha.8]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.8
[0.1.0-alpha.7]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.7
[0.1.0-alpha.6]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.6
[0.1.0-alpha.5]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.5
[0.1.0-alpha.4]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.4
[0.1.0-alpha.3]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.3
[0.1.0-alpha.2]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.2
[0.1.0-alpha.1]: https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.1
