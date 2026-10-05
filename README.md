<div align="center">

# CrabXL

**Read, write, and edit XLSX files in Rust.**

[![Crates.io](https://img.shields.io/crates/v/crabxl?include_prereleases)](https://crates.io/crates/crabxl)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange?logo=rust)](https://doc.rust-lang.org/cargo/reference/rust-version.html)
[![CI](https://github.com/crabxl/crabxl/actions/workflows/rust.yml/badge.svg)](https://github.com/crabxl/crabxl/actions/workflows/rust.yml)
[![License](https://img.shields.io/github/license/crabxl/crabxl)](LICENSE)

[API documentation](https://docs.rs/crabxl) · [Python package](https://github.com/crabxl/crabxl-python) · [Report an issue](https://github.com/crabxl/crabxl/issues)

</div>

## About

CrabXL provides bounded streaming reads, sequential writes, and existing-file
edits through a shared Rust engine. It targets the public capabilities of
openpyxl while keeping the Rust API idiomatic.

- Stream sparse worksheet rows or explicitly load an owned sheet.
- Read exact integers, text, rich text, dates, styles, and formula records.
- Create workbooks and edit supported cell values while preserving unrelated
  original package parts.
- Configure memory allowances, shared-string RAM/disk storage, caches, resource
  limits, and ZIP compression.

**Currently alpha.** Full editing and openpyxl feature coverage are still in
progress. Unsupported operations return explicit errors. Formula calculation is
not provided. See the [capability inventory](docs/features.json) and
[roadmap](docs/roadmap.md) for current scope.

## Installation

Requires **Rust 1.88 or newer**.

```toml
[dependencies]
crabxl = "0.1.0-alpha.5"
```

## Usage

### Read rows

```rust
use crabxl::WorkbookReader;

fn main() -> crabxl::Result<()> {
    let mut workbook = WorkbookReader::open("input.xlsx")?;
    let mut rows = workbook.rows("Sheet1")?;

    while let Some(row) = rows.next_row()? {
        for cell in row.cells {
            println!("{:?}: {:?}", cell.address, cell.value);
        }
    }
    Ok(())
}
```

Coordinates are zero-based. Streaming returns physically present cells with their
original positions. Use `read_sheet` when you need an owned worksheet snapshot.

### Edit an existing file

```rust
use crabxl::{CellAddress, CellValue, SaveOptions, WorkbookEditor};

fn main() -> crabxl::Result<()> {
    let mut workbook = WorkbookEditor::open("input.xlsx")?;
    workbook.set_value("Sheet1", CellAddress::new(0, 0)?, CellValue::Integer(42))?;
    workbook.save_path("output.xlsx", SaveOptions::default())?;
    Ok(())
}
```

Use `WorkbookWriter` for sequential creation. Runnable examples are available in
[crates/crabxl/examples](crates/crabxl/examples).

## Resource and compression options

`ResourceLimits` bounds input, metadata, cells, rows, and batches.
`MemoryPolicy` offers explicit budgets or configurable Auto selection;
`SharedStringOptions` controls RAM/disk placement and decoded caches. Managed
allowances are not a whole-process RSS cap.

Write and save options accept compression levels **0–9**: 0 stores without
compression; 1–9 use Deflate; the default is 6. The default backend is pure-Rust
zlib-rs. Native zlib is available through the `deflate-zlib` feature with default
features disabled. Existing-file saves retain compressed bytes for untouched
parts.

## Documentation

- [API reference](https://docs.rs/crabxl)
- [Roadmap](docs/roadmap.md) and [capability inventory](docs/features.json)
- [Architecture](docs/architecture.md)
- [Benchmarks](benchmarks/README.md)
- [Releases](docs/releases.md)

## License

Distributed under the [MIT License](LICENSE). Third-party notices and source
provenance are listed in [third_party](third_party).
