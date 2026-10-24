---
title: CrabXL for Rust
description: Install the crabxl crate and read a worksheet.
---

## Install

Add the crate to `Cargo.toml`. The minimum Rust version is 1.88.

```toml
[dependencies]
crabxl = "0.1.0-alpha.11"
```

## Read rows

```rust
use crabxl::WorkbookReader;

fn main() -> crabxl::Result<()> {
    let mut workbook = WorkbookReader::open("input.xlsx")?;
    let mut rows = workbook.rows("Sheet1")?;
    while let Some(row) = rows.next_row()? {
        for cell in row.cells {
            println!("{}: {:?}", cell.address, cell.value);
        }
    }
    Ok(())
}
```

`rows` returns only the cells that are in the file. Row and column indexes start
at zero.

The [quickstart example](https://github.com/crabxl/crabxl/blob/main/crates/crabxl/examples/quickstart.rs)
also creates and edits a workbook. The [API reference](https://docs.rs/crabxl)
lists all types.

## Limits

- An unsupported operation returns an error of kind `ErrorKind::Unsupported`.
- CrabXL does not calculate formulas. A formula cell contains the formula and
  the cached result from the file.
