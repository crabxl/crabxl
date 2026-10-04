# crabxl

A Rust spreadsheet library focused on fast, memory-efficient XLSX processing and full coverage of the public openpyxl feature baseline. Internal implementation and Rust API naming may be idiomatic Rust; external capabilities and observable behavior must remain complete.

M1 provides bounded sparse XLSX row streaming and explicit owned sheet materialization. M2 adds exact integers, plain inline/shared/value text, booleans/errors, bounded shared-string storage, explicit typed rich text, shared style catalogs, numeric dates/time/durations and owned-payload budgets. M3 sequential writer acceptance is complete: scalars, dates/time/duration, normal formulas/caches and basic styles use shared core types and selected rust_xlsxwriter codecs. Complete theme/style editing, advanced formulas, complete editing/preservation, full Python compatibility and other language bindings remain staged.

Rust 1.88.0 or later is required. Run the example against an unstyled numeric worksheet:

```sh
cargo run --release -p crabxl --example sum -- numbers.xlsx Sheet
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

```rust
use crabxl::{CellValue, Row, RowIndex, WorkbookReader};

fn sum(path: &str, sheet: &str) -> crabxl::Result<f64> {
    let mut workbook = WorkbookReader::open(path)?;
    let mut rows = workbook.rows(sheet)?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut total = 0.0;
    while rows.read_row_into(&mut row)? {
        for cell in &row.cells {
            let value = match cell.value {
                CellValue::Number(value) => Some(value),
                CellValue::Integer(value) if (-9_007_199_254_740_992..=9_007_199_254_740_992).contains(&value) => Some(value as f64),
                CellValue::Integer(_) | CellValue::BigInteger(_) => return Err(crabxl::Error::new(
                    crabxl::ErrorKind::Unsupported, "This f64 sum rejects integers beyond 2^53"
                ).with_cell(cell.address)),
                _ => None,
            };
            if let Some(value) = value {
                total += value;
            }
        }
    }
    Ok(total)
}
```

Coordinates are zero-based typed indices with explicit A1 conversions. Missing rows/columns are not expanded; returned cells retain their actual positions. `rows_with_options` projects before scalar decoding. `next_row` and `read_batch` return owned data; `read_row_into` reuses an allocation. The workbook owns a seekable source and each reader borrows it exclusively.

Use `read_sheet` for a non-streaming public operation: it retains all supported sparse scalar rows in owned `SheetData`, allowing repeated access without reparsing. It uses the same incremental decoder internally; materialization does not itself accelerate the first read. Configure the data budget and input buffer independently:

```rust
use crabxl::{ResourceLimits, WorkbookReader};

fn load(path: &str, sheet: &str) -> crabxl::Result<crabxl::SheetData> {
    let limits = ResourceLimits {
        max_materialized_bytes: 1024 * 1024 * 1024,
        input_buffer_bytes: 256 * 1024,
        ..ResourceLimits::default()
    };
    let mut workbook = WorkbookReader::open_with_limits(path, limits)?;
    workbook.read_sheet(sheet)
}
```

The default direct `read_sheet` retained-data budget is 256 MiB. Parser/catalog memory and one current row are additional. A budget failure returns `MemoryBudgetExceeded`, discards partial output, and releases the reader. `SheetData` is a numeric snapshot, not a saveable editable workbook. Current component budgets are not a hard process RSS limit.

For automatic numeric mode selection use `read_with_policy`. A scan streams; repeated access samples at most 128 rows and retains data when its estimate fits. If later rows exceed the actual allowance, partial materialization is discarded and the operation returns a fresh stream. Other input errors propagate. Inspect `output.decision` for budget, estimate, source, mode, and reason:

```rust
use crabxl::{AccessPattern, MemoryPolicy, ReadData, WorkbookReader};

fn count_rows(path: &str, sheet: &str) -> crabxl::Result<usize> {
    let mut workbook = WorkbookReader::open(path)?;
    let output = workbook.read_with_policy(
        sheet, AccessPattern::RepeatedAccess, MemoryPolicy::default(),
    )?;
    match output.data {
        ReadData::Materialized(data) => Ok(data.rows.len()),
        ReadData::Streaming(mut rows) => rows.try_fold(0, |count, row| row.map(|_| count + 1)),
    }
}
```

`MemoryPolicy::Budget(bytes)` specifies an operation ceiling. `MemoryPolicy::Auto(AutoMemory { .. })` tunes availability fraction, headroom, maximum budget, and a caller-supplied availability override. Default Auto uses 25% of effective availability after headroom. Linux probing includes host `MemAvailable`, cgroup v2 ancestors, and finite address/data limits. Other platforms, cgroup v1, and incomplete probes use a conservative fallback; callers can supply effective availability. The operation reserves parser working space and gives the remainder to retained row/vector capacities. Existing catalog, dependency/allocator overhead, and caller-retained outputs are additional: this is not a hard process RSS cap or a reservation against other processes.

Auto overrides the retained-data allowance only for its operation; direct `rows` and `read_sheet` retain their explicit semantics. It keeps the configured input buffer instead of assuming larger buffers improve CPU-bound parsing. Adaptive string/style caches, concurrency and the full editable mode remain future work; see [ADR 0002](docs/decisions/0002-adaptive-memory.md).

This checkpoint preserves signed i64 integers and larger exact decimal integers, floating-point literals including scientific overflow infinities, booleans, plain inline/shared/value text, spreadsheet error tokens, and physically present empty cells. Owned string/error/big-integer payloads are included in row, batch, materialization, and Auto allocation estimates. ReadOptions.rich_text selects owned rich runs, optional font overrides, color references and phonetic annotations; false explicitly projects displayed text without formatting allocations. Normal/shared expressions and array/data-table records use typed core formula models; shared expansion is bounded and sparse, including projected template dependencies. Read-side style catalogs interpret numeric dates, clocks and durations, including style zero. cm/vm metadata graphs, complete theme/catalog editing and aggregate adaptive budgets remain staged. See ADR 0011 and ADR 0014 for verified scope. Inline OOXML-looking literals retain their spelling; shared strings remove the pinned reference protection marker `x005F_` without decoding arbitrary `_xHHHH_` sequences. Typed inline/shared reads remove protection per run or lone text element, while plain SST projection removes it after flattening and default inline projection retains it. See [ADR 0010](docs/decisions/0010-rich-text-projections.md) for mode upgrades, disk-backed rich payloads and imported phonetic-font assignment limits. This is not a general openpyxl replacement yet.

Shared strings resolve through workbook relationships and are prepared once on first row access, without materializing a worksheet. `set_shared_string_options` selects `SharedStringStorage::Memory`, `Disk`, or default `Auto`, a shared `MemoryPolicy`, byte-bounded decoded cache, temporary-byte/entry limits and temporary directory. Auto starts in RAM and spills both payloads and the fixed-width index when actual usage exceeds its allowance. Disk mode retains neither the whole table nor its index in RAM. `shared_string_stats()` reports placement, policy/retained budgets, allocations, temporary bytes and cache hits/reads. Rows own their text independently of table/cache lifetime.

String component budgets reserve parser working space; catalogs, retained rows, dependency allocations and OS file cache are additional, and simultaneous component budgets are not yet a global RSS cap. Preparing the entire SST verifies its XML/CRC even for projected reads. Rich/phonetic entries retain their IDs but explicitly reject selected access until typed support lands; selecting unrelated plain entries is supported. Tables are released on workbook Drop or option reconfiguration. See [shared-string measurements](benchmarks/m2-shared-strings.md).

Configurable `ResourceLimits` bound archive size, metadata, XML input/events/depth, values, rows, and batches. Memory includes the ZIP catalog and metadata; user-retained batches add memory. Full consumption checks XML and entry CRC; dropping a reader early releases it without validating unread bytes. See [ownership and memory details](docs/decisions/0001-numeric-streaming.md) and [measured benchmarks](benchmarks/README.md).

- [Architecture](docs/architecture.md)
- [Roadmap and feature inventory](docs/roadmap.md)
- [Upstream sources](docs/upstream-sources.md)
- [Verified capability inventory](docs/features.json)
- [Pending openpyxl fixes and regression risks](docs/openpyxl-mr-review.md)
- [AI agent instructions](AGENTS.md)

Boolean values remain distinct from numeric zero/one in rows, batches, materialization, and Auto mode. Decimal integer boolean literals follow the baseline zero/nonzero behavior without integer overflow; invalid boolean text returns a contextual error. `scalar_counts` counts these types without retaining a sheet.

The sum example is a controlled f64 checksum benchmark: it rejects integer inputs beyond 2^53 instead of rounding or skipping them. Core values retain exact integers independently of that example. Mixed scalar measurements are in [benchmarks/m2-scalars.md](benchmarks/m2-scalars.md).

Sequential creation uses the same core values, addresses and styles:

```rust
use crabxl::{Cell, CellAddress, CellValue, Row, RowIndex, StyleId, WorkbookWriter, WriteOptions};

let mut writer = WorkbookWriter::new(WriteOptions::default())?;
writer.start_sheet("Sheet")?;
let mut row = Row::new(RowIndex::new(0)?);
row.cells.push(Cell { address: CellAddress::new(0, 0)?, value: CellValue::Integer(42), style: StyleId::new(0) });
writer.write_row(&row)?;
writer.finish(std::fs::File::create("output.xlsx")?)?;
```

Earlier rows/sheets cannot be revisited in this mode. `abort()` and Drop clean owned temporary files without publishing; `finish()` explicitly packages the workbook and may leave partial output on I/O failure. Use `WriteOptions` for byte limits and temporary-directory selection. The M3 writer supports scalar data, date/time/duration values in both epochs, normal formulas with optional typed caches, and registered complete appearance components (optional fonts, pattern/gradient fills, nine border positions, alignment, number format and cell protection). Inline escape-looking literals now retain their public reference spelling. See [writer ownership decisions](docs/decisions/0004-sequential-scalar-writer.md) and [writer measurements](benchmarks/m3-writer.md).

Register a `CellStyle` once with `writer.register_style(style)` and reuse its `StyleId` in cells. Equal formats reuse IDs; the table has count/metadata limits. ID zero is General for writer-created scalar cells. Date values with ID zero automatically receive a date/time/duration format; explicitly styled dates require an appropriate number format. `ExcelDateTime::from_ymd_hms_milli` validates calendar fields; `from_serial` retains an exact serial and source epoch. Serial 60 is retained in the Windows epoch and rejected on conversion to the Mac epoch rather than silently changing its meaning. Imported calendar serial conversion follows the baseline's millisecond rounding and maps Windows serial 60 to February 28. Literal constructors preserve original Gregorian identity and microseconds, while imported early-date serials remain ambiguous.

`Formula::new("=SUM(A1:A2)", None)` writes a formula without a fabricated cache. Supply a typed `CellValue` to retain zero, false, text, error or date results; this crate does not calculate formulas. The streaming reader also returns normal `CellValue::Formula` values and their caches; `ReadOptions { data_only: true, ..Default::default() }` requests cached values, returning Empty when absent. A supplied Empty cache is serialized as absent. Formula strings and cache payloads count toward retained-data budgets. Numeric styled values and caches now use the imported catalog. Shared/array/data-table formulas, theme resolution, full named/differential styles and existing-file style editing remain in M2/M4/M5.

## Existing-file edits and sparse models (M4 checkpoint)

`WorkbookEditor::open` retains a seekable original package. `set_value` queues an existing-cell replacement, preserving its style and unrelated original parts; physical existence and metadata are checked on save. `save_path` atomically replaces a target after successful output, using a full adjacent temporary ZIP. Repeated saves reuse the original source without resident copies of images. `clear_edits` restores that original baseline. Old formula caches are removed across worksheets and full recalculation requested.

```rust
use crabxl::{CellAddress, CellValue, SaveOptions, WorkbookEditor};

fn edit(source: &str, target: &str) -> crabxl::Result<()> {
    let mut workbook = WorkbookEditor::open(source)?;
    workbook.set_value("Sheet", CellAddress::new(0, 0)?, CellValue::Integer(42))?;
    workbook.save_path(target, SaveOptions::default())?;
    Ok(())
}
```

`EditorOptions` offers Auto/explicit memory policy and patch byte/cell caps; resolved allowances are inspectable. `SaveOptions::verify_unchanged` enables full CRC checking of unchanged parts, at decompression cost. The default compressed copy does not validate their payload CRC. Core `Worksheet` separately provides sparse random access, append and bounded insert/delete/move/copy operations; `WorkbookWriter::write_worksheet` exports borrowed cells into a new package.

M4 remains in progress. Existing-file structural edits, sheet mutations and loaded catalog mutation and graph integration remain staged. Unsafe cell metadata and signed-package edits are rejected. Conventional derived calculation chains are discarded on edits with synchronized package references; an explicit policy can reject chain edits. See [ownership and limitations](docs/decisions/0005-sparse-preserving-editor.md) and [Rust/openpyxl edit measurements](benchmarks/m4-editor.md).

For missing cells use `WorkbookEditor::upsert_value`; it inserts sparse cells/rows with default style, keeps inferred original positions and expands an existing dimension. Non-anchor merged targets are rejected. [Insertion measurements](benchmarks/m4-insertion.md) include public readback and temporary-output costs.

## Optional Python compatibility adapter

The adapter uses openpyxl call conventions; migrating supported code changes the import:

```python
import crabxl as openpyxl

wb = openpyxl.Workbook()
ws = wb.active
ws["A1"] = 123
ws.append([True, "text", "=A1+1"])
wb.save("example.xlsx")
```

Build/install/test instructions and the explicit capability limits are in [https://github.com/crabxl/crabxl-python/blob/main/README.md](https://github.com/crabxl/crabxl-python/blob/main/README.md). The standalone Rust crate has no Python dependency. Compatibility is partial and verified by selected original openpyxl tests plus shared public-API cases; advanced features remain in the roadmap.

Core `Workbook` now provides stable sheet IDs, order/active/epoch selection, independent sparse model copies and aggregate managed allowances. `sheet_mut` returns a guarded mutation facade; `WorkbookWriter::write_workbook` exports borrowed models. This does not implement original-package sheet or feature-graph surgery. See [ADR 0007](docs/decisions/0007-owned-workbook.md) and [release evidence](benchmarks/m4-workbook.md).

The Python owned Workbook now shares the Rust aggregate model allowance and supports compatible `copy_worksheet`, `move_sheet`, `index` and deletion calls. Removed retained worksheets remain usable. Loaded models still have per-model/overlay allowances. [Same-call copy/export evidence](benchmarks/m4-python-bank.md) reports performance, memory and temporary-storage tradeoffs.

[Calculation-chain policy](docs/decisions/0008-derived-calculation-chain.md) preserves unchanged chains and removes obsolete chain parts, content types and workbook relationships on edited saves. Unknown consumers and unsafe graphs reject edits.

## Imported styles and numeric dates

`WorkbookReader::style_catalog()` lazily exposes shared component tables, original cell/base-format IDs, named-style metadata and indexed/recent colors. Fonts use the same optional fields as rich-run fonts. Pattern/gradient fills, nine border positions, alignment and protection preserve absent versus explicit values. `max_style_bytes` bounds retained catalog/classification capacities; `max_style_records` bounds actual entries per table. These are additional component allowances, not a hard process RSS ceiling.

Numeric cells and numeric formula caches with date formats produce `ExcelDateTime` in the source epoch. Clock fractions and elapsed durations retain distinct kinds. Default `DateReadPolicy::Compatible` follows the pinned baseline's millisecond rounding and returns `#VALUE!` for unrepresentable dates/durations. `RetainSerial` preserves finite source serials as an extension. Use `rows_with_options` or `read_sheet_with_options` for the same policies in streaming or materialized mode. Booleans/text/errors retain their types regardless of date formatting.

Theme resolution, full imported style editing and aggregate catalog accounting remain required. Unmodeled catalog sections are listed explicitly and survive through original-package preservation, not typed catalog export. See [catalog decisions](docs/decisions/0011-style-catalog-and-date-reading.md) and [numeric-date/style evidence](benchmarks/m2-styles-dates.md).

`ExcelDateTime::from_ymd_hms_micro`, `from_hms_micro` and `from_duration_parts` preserve literal microsecond values. Their conversion methods return exact literal components; `from_serial` retains numeric source serials with loaded baseline millisecond conversion. `serial_in` converts original literal Gregorian days directly, including early Windows ambiguities. See [literal precision decisions](docs/decisions/0012-literal-date-precision.md) and [creation/read evidence](benchmarks/m2-date-literals.md).

`ExcelDateTime::from_ymd` retains date-only Gregorian values. Shared `parse_iso8601`/`to_iso8601` support the pinned public ISO date/clock/elapsed-duration behavior, including documented prefix acceptance and millisecond truncation. `WriteOptions { iso_dates: true, ..Default::default() }` writes date/calendar/clock cells as ISO values in either workbook epoch; elapsed durations remain numeric. Default writer styles include a date-only format. See [ISO compatibility decisions](docs/decisions/0013-iso-date-compatibility.md) and [ISO creation/read measurements](benchmarks/m2-iso-dates.md).

Nonfinite numeric literals/caches use `NonFiniteWritePolicy::Blank` by default, matching public blank serialization; select `Reject` for atomic strict validation. This policy is shared by sequential output and original-package overlays. Native nonfinite compatibility does not imply nonfinite date/style support or exact nonfinite round-trip storage. See ADR 0015.
