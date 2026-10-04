# openrsxl

A Rust spreadsheet library focused on fast, memory-efficient XLSX processing and full coverage of the public openpyxl feature baseline. Internal implementation and Rust API naming may be idiomatic Rust; external capabilities and observable behavior must remain complete.

The first M1 checkpoint implements bounded, sparse numeric XLSX row streaming and explicit owned sheet materialization. It ports selected calamine parsing logic into shared Rust models rather than wrapping calamine. Full spreadsheet support remains in the roadmap; writers, editing, other value types, styles, and language bindings are not implemented yet.

Rust 1.88.0 or later is required. Run the example against an unstyled numeric worksheet:

```sh
cargo run --release -p openrsxl --example sum -- numbers.xlsx Sheet
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

```rust
use openrsxl::{CellValue, Row, RowIndex, WorkbookReader};

fn sum(path: &str, sheet: &str) -> openrsxl::Result<f64> {
    let mut workbook = WorkbookReader::open(path)?;
    let mut rows = workbook.rows(sheet)?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut total = 0.0;
    while rows.read_row_into(&mut row)? {
        for cell in &row.cells {
            if let CellValue::Number(value) = cell.value {
                total += value;
            }
        }
    }
    Ok(total)
}
```

Coordinates are zero-based typed indices with explicit A1 conversions. Missing rows/columns are not expanded; returned cells retain their actual positions. `rows_with_options` projects before numeric decoding. `next_row` and `read_batch` return owned data; `read_row_into` reuses an allocation. The workbook owns a seekable source and each reader borrows it exclusively.

Use `read_sheet` for a non-streaming public operation: it retains all sparse numeric rows in owned `SheetData`, allowing repeated access without reparsing. It uses the same incremental decoder internally; materialization does not itself accelerate the first read. Configure the data budget and input buffer independently:

```rust
use openrsxl::{ResourceLimits, WorkbookReader};

fn load(path: &str, sheet: &str) -> openrsxl::Result<openrsxl::SheetData> {
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
use openrsxl::{AccessPattern, MemoryPolicy, ReadData, WorkbookReader};

fn count_rows(path: &str, sheet: &str) -> openrsxl::Result<usize> {
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

This checkpoint reads raw finite `f64` numbers and physically present empty cells. Integer literals beyond exact `f64` precision may round; exact baseline integer semantics remain a later value-model requirement. Selected strings, booleans, errors, formulas, nonzero style indices, and cm/vm metadata return `Unsupported`. Style tables are not loaded: style index zero is treated as raw numeric data even if an unusual workbook customizes its formatting. Date/style interpretation belongs to M2; use M1 only for known unstyled numeric input. This is not a general openpyxl replacement yet.

Configurable `ResourceLimits` bound archive size, metadata, XML input/events/depth, values, rows, and batches. Memory includes the ZIP catalog and metadata; user-retained batches add memory. Full consumption checks XML and entry CRC; dropping a reader early releases it without validating unread bytes. See [ownership and memory details](docs/decisions/0001-numeric-streaming.md) and [measured benchmarks](benchmarks/README.md).

- [Architecture](docs/architecture.md)
- [Roadmap and feature inventory](docs/roadmap.md)
- [Upstream sources](docs/upstream-sources.md)
- [Verified capability inventory](docs/features.json)
- [Pending openpyxl fixes and regression risks](docs/openpyxl-mr-review.md)
- [AI agent instructions](AGENTS.md)
