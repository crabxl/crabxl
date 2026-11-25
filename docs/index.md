---
title: CrabXL for Rust
description: Install the crabxl crate, then create, edit and stream workbooks.
---

## Install

Add the crate to `Cargo.toml`. The minimum Rust version is 1.88.

```toml
[dependencies]
crabxl = "0.3.0"
```

## Choose an entry point

| Type | Use it to | Memory |
|---|---|---|
| `Workbook` | Create a workbook, or open one and change it. | The sheets you use. |
| `read::Reader` | Read large files row by row. | One row. |
| `write::Writer` | Write large files row by row. | One row. |

All three use the same values, styles, references and errors.

## Create and edit

```rust
use crabxl::style::{Font, Style};
use crabxl::{Formula, Workbook};

fn main() -> crabxl::Result<()> {
    let mut book = Workbook::new()?;
    let mut sheet = book.add_worksheet("Sales")?;
    sheet.append_row(("Region", "Q1", "Q2", "Total"))?;
    sheet.append_row(("North", 120.5, 98.0, Formula::new("B2+C2")?))?;
    sheet.set_style("A1:D1", &Style::new().with_font(Font::new().with_bold(true)))?;
    book.save("sales.xlsx")?;

    let mut book = Workbook::open("sales.xlsx")?;
    let mut sheet = book.worksheet("Sales")?;
    sheet.insert_rows("2")?;
    sheet.set_values("A2", [("South", 80, 95)])?;
    book.save("sales.xlsx")
}
```

After the insert, the formula in `D3` reads `B3+C3`. Inserting or deleting
rows or columns updates every reference to the moved cells. References into
deleted cells become `#REF!`.

## Values

A cell holds a `Value` and, optionally, a `Formula`. For a formula cell, the
value is the cached result.

| `Value` | Contents |
|---|---|
| `Blank` | An empty cell, or a formula without a cached result. |
| `Bool` | `TRUE` or `FALSE`. |
| `Number` | A number. It keeps its exact text when `f64` cannot hold it. |
| `Text` | Text. Text that starts with `=` stays text. |
| `RichText` | Text with formatted runs. |
| `Temporal` | A date, date and time, time or duration, with its exact serial. |
| `Error` | A cell error such as `#DIV/0!`. |

Convert values with `TryFrom` (`let n: f64 = (&value).try_into()?`) and rows
with `FromRow` (`sheet.rows_as::<(String, f64)>("A2:B9")?`). A mismatch
returns an error of kind `ErrorKind::Conversion`.

## Stream rows

```rust
use crabxl::read::{Reader, RowOptions};

fn main() -> crabxl::Result<()> {
    let mut reader = Reader::open("events.xlsx")?;
    let options = RowOptions::new().with_columns("A:C")?.with_skip_rows(1);
    for row in reader.rows("Events", &options)? {
        let (id, kind, amount): (i64, String, f64) = row?.parse()?;
        println!("{id} {kind} {amount}");
    }
    Ok(())
}
```

`write::Writer` writes rows in the same way. Set column widths and frozen
panes before the first row.

## Calculate formulas

Turn on the `calc` feature to calculate formulas with the
[Formualizer](https://github.com/psu3d0/formualizer) engine.

```toml
[dependencies]
crabxl = { version = "0.3.0", features = ["calc"] }
```

`Workbook::calculate` stores each result as the formula's cached value, so
programs that do not calculate formulas, such as pandas, see current results.
Call it right before you save. Array and data-table formulas are not
calculated.

## Saving without data loss

A save copies every part you did not change, byte for byte. When a save would
remove content that CrabXL cannot keep, it returns an error of kind
`ErrorKind::Unsupported` that names the content. Examples are unknown
attributes in a feature you changed, or a digital signature that an edit
invalidates. To save without that content, open the workbook with
`OpenOptions::new().with_data_loss(DataLossPolicy::Allow)`.

Saves to a path are atomic: the file is replaced completely or left unchanged.

## Threads

Large worksheets are read, saved and compressed on several threads. CrabXL
uses at most the CPUs the process may run on. To use fewer, set the
`CRABXL_MAX_THREADS` environment variable or call `crabxl::set_max_threads`,
counting the calling thread. `1` keeps all work on the calling thread.

## Untrusted files

Input limits guard against files built to exhaust memory or CPU. They apply
when reading and editing, never to data you write. The default opens every
file Excel writes and stops ZIP bombs, deeply nested XML and huge XML tokens.
Use `Limits::Strict` for uploads from strangers and `Limits::Unlimited` for
files you trust:

```rust
use crabxl::{Limits, OpenOptions, Workbook};

fn main() -> crabxl::Result<()> {
    let options = OpenOptions::new().with_limits(Limits::Strict);
    let book = Workbook::open_with("upload.xlsx", &options)?;
    println!("{:?}", book.sheet_names());
    Ok(())
}
```

A file that exceeds a limit returns an error of kind
`ErrorKind::LimitExceeded`. XML document types are always rejected.

## Limits

- Without the `calc` feature, CrabXL does not calculate formulas. A formula
  cell has the formula and the cached result from the file.
- Pivot tables, scenarios, array formulas and 3-D references cannot follow
  row and column edits. Such an edit returns an error before anything
  changes.
