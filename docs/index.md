---
title: CrabXL for Rust
description: Read, write and edit XLSX files in Rust.
---

CrabXL reads, writes and edits XLSX files. It streams rows with bounded memory,
creates workbooks sequentially and edits existing files. Parts of a file that
you do not change are kept without change.

CrabXL is alpha software. Unsupported operations return an `Unsupported`
error. CrabXL does not calculate formulas.

## Install

Add CrabXL to `Cargo.toml`. CrabXL requires Rust 1.88 or later.

```toml
[dependencies]
crabxl = "0.1.0-alpha.11"
```

## Next steps

- Run the [quickstart example](https://github.com/crabxl/crabxl/blob/main/crates/crabxl/examples/quickstart.rs).
- Read the [API reference](https://docs.rs/crabxl).

More pages are in preparation.
