<div align="center">

# CrabXL

**Read, write, and edit XLSX files in Rust.**

[![Crates.io](https://img.shields.io/crates/v/crabxl)](https://crates.io/crates/crabxl)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange?logo=rust)](https://doc.rust-lang.org/cargo/reference/rust-version.html)
[![CI](https://github.com/crabxl/crabxl/actions/workflows/rust.yml/badge.svg)](https://github.com/crabxl/crabxl/actions/workflows/rust.yml)
[![License](https://img.shields.io/github/license/crabxl/crabxl)](LICENSE)

[API documentation](https://docs.rs/crabxl) · [Python package](https://github.com/crabxl/crabxl-python) · [Report an issue](https://github.com/crabxl/crabxl/issues)

</div>

## About

CrabXL reads, writes and edits XLSX files in Rust. Three entry points cover
the three ways of working with a file, and all three share one model of
values, formulas, styles and references:

| Type | Use it to | Memory |
|---|---|---|
| `Workbook` | create a workbook, or open one and change it | the sheets you use |
| `read::Reader` | read large files row by row | one row |
| `write::Writer` | write large files row by row | one row |

- Edits are lossless. A save copies every part you did not change. It fails
  instead of dropping content that CrabXL cannot keep, unless you allow it.
- Saves to a path are atomic. Saving over the file you opened is safe.
- Inserting or deleting rows or columns updates every reference: formulas on
  all sheets, names, tables, rules, merges, charts and drawings.
- Numbers keep their full precision. Text never turns into a formula.
- Invalid input returns explicit errors with the sheet and cell.

Before version 1.0, a minor version can change the API. The optional `calc`
feature calculates formulas with the Formualizer engine and stores the
results as cached values. See [project status](internal/STATUS.md).

## Installation

Requires **Rust 1.88 or newer**.

```toml
[dependencies]
crabxl = "0.4.1"
```

## Usage

### Create a workbook

```rust
use crabxl::style::{Color, Fill, Font, Style};
use crabxl::{Formula, Workbook};

fn main() -> crabxl::Result<()> {
    let header = Style::new()
        .with_font(Font::new().with_bold(true))
        .with_fill(Fill::solid(Color::hex("DDEBF7")?));

    let mut book = Workbook::new()?;
    let mut sheet = book.add_worksheet("Sales")?;
    sheet.append_row(("Region", "Q1", "Q2", "Total"))?;
    sheet.append_row(("North", 120.5, 98.0, Formula::new("B2+C2")?))?;
    sheet.set_style("A1:D1", &header)?;
    sheet.set_column_width("A:D", 14.0)?;
    sheet.freeze_panes("A2")?;
    book.save("sales.xlsx")
}
```

### Edit a file

```rust
use crabxl::Workbook;

fn main() -> crabxl::Result<()> {
    let mut book = Workbook::open("report.xlsx")?;
    let mut summary = book.worksheet("Summary")?;
    let total: f64 = (&summary.value("B10")?).try_into()?; // the cached result
    summary.insert_rows("5:6")?; // formulas, names and ranges follow the cells
    summary.set_value("B5", total * 2.0)?;
    summary.map_style("B2:B12", |style| style.with_number_format("#,##0.00"))?;
    book.save("report.xlsx") // atomic, even over the source
}
```

A cell holds a `Value` and, optionally, a `Formula`; for a formula cell the
value is the cached result. `rows_as::<(String, f64)>("A2:B9")` converts rows
to tuples, and `values("A1:D10")` returns a dense `Grid`.

### Stream rows

```rust
use crabxl::read::{Reader, RowOptions};
use crabxl::write::{WriteOptions, Writer};

fn main() -> crabxl::Result<()> {
    let mut reader = Reader::open("events.xlsx")?;
    let options = RowOptions::new().with_columns("A:C")?.with_skip_rows(1);
    let mut writer = Writer::create("totals.xlsx", &WriteOptions::new())?;
    let mut sheet = writer.add_worksheet("Totals")?;
    for row in reader.rows("Events", &options)? {
        let (id, kind, amount): (i64, String, f64) = row?.parse()?;
        sheet.append_row((id, kind, amount * 1.2))?;
    }
    writer.finish()
}
```

The [quickstart example](crates/crabxl/examples/quickstart.rs) runs all three:

```sh
cargo run -p crabxl --example quickstart
```

## Options

`OpenOptions` sets input safety limits (`Limits::Strict` for untrusted
uploads, `Limits::Unlimited` for files you trust), a memory budget
(`Memory::Budget(bytes)`), the data-loss policy and exact-number reading.
The memory budget covers loaded sheets, edits and shared strings; it is not
a cap on process memory.

`SaveOptions` and `WriteOptions` accept compression levels **0–9**: 0 stores
without compression, 1–9 use Deflate, and the default is 6. Worksheets of
4 MiB or more compress on up to four threads. The default backend is
pure-Rust zlib-rs; native zlib is available through the `deflate-zlib`
feature with default features disabled.

## Crates

| Crate | Contents |
|---|---|
| `crabxl` | The public API described here. |
| `crabxl-xlsx` | The XLSX engine: package, readers, writers, the preserving editor. |
| `crabxl-core` | I/O-free models: values, styles, features, references. |
| `crabxl-calc` | Optional formula calculation. |

Language bindings and tools that need engine-level access depend on
`crabxl-xlsx` and `crabxl-core` directly.

## Documentation

- [API reference](https://docs.rs/crabxl)
- [Project status](internal/STATUS.md)
- [Changelog](CHANGELOG.md)

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request. Report
security problems as described in [SECURITY.md](SECURITY.md).

## License

Distributed under the [MIT License](LICENSE). Third-party notices and source
provenance are listed in [third_party](third_party).
