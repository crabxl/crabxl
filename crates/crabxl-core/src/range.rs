// SPDX-License-Identifier: MIT
// Axis-optional range layout adapted from umya-spreadsheet.
// Copyright (c) 2020 MathNya. Provenance: third_party/ports.json.
//! Allocation-free geometry for finite rectangles and complete worksheet axes.

use crate::{CellAddress, CellRange, ColumnIndex, Error, ErrorKind, Result, RowIndex};
use std::{fmt, str::FromStr};

/// A worksheet-local range without allocating its covered cells.
/// Absolute markers are accepted when parsing; geometry does not retain them.
/// Sheet qualification belongs to the owning feature, rather than this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorksheetRange(Extent);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Extent {
    Cells(CellRange),
    Rows(RowIndex, RowIndex),
    Columns(ColumnIndex, ColumnIndex),
}

impl WorksheetRange {
    /// Construct validated finite geometry.
    pub fn cells(range: CellRange) -> Result<Self> {
        Ok(Self(Extent::Cells(CellRange::new(range.start, range.end)?)))
    }

    /// Cover every column between two inclusive, ordered rows.
    pub fn rows(start: RowIndex, end: RowIndex) -> Result<Self> {
        if start > end {
            return Err(invalid());
        }
        Ok(Self(Extent::Rows(start, end)))
    }

    /// Cover every row between two inclusive, ordered columns.
    pub fn columns(start: ColumnIndex, end: ColumnIndex) -> Result<Self> {
        if start > end {
            return Err(invalid());
        }
        Ok(Self(Extent::Columns(start, end)))
    }

    /// Resolve legal worksheet bounds without materializing any cells.
    pub fn bounds(self) -> CellRange {
        match self.0 {
            Extent::Cells(range) => range,
            Extent::Rows(start, end) => CellRange {
                start: CellAddress {
                    row: start,
                    column: ColumnIndex::FIRST,
                },
                end: CellAddress {
                    row: end,
                    column: ColumnIndex::LAST,
                },
            },
            Extent::Columns(start, end) => CellRange {
                start: CellAddress {
                    row: RowIndex::FIRST,
                    column: start,
                },
                end: CellAddress {
                    row: RowIndex::LAST,
                    column: end,
                },
            },
        }
    }

    /// Test geometric membership independently of physical cell presence.
    pub fn contains(self, address: CellAddress) -> bool {
        self.bounds().contains(address)
    }

    /// Whether all columns are covered by an explicit whole-row range.
    pub const fn is_rows(self) -> bool {
        matches!(self.0, Extent::Rows(..))
    }

    /// Whether all rows are covered by an explicit whole-column range.
    pub const fn is_columns(self) -> bool {
        matches!(self.0, Extent::Columns(..))
    }

    /// Geometric coverage, which can exceed a 32-bit process's address space.
    pub fn cell_count(self) -> u64 {
        let range = self.bounds();
        u64::from(range.end.row.get() - range.start.row.get() + 1)
            * u64::from(range.end.column.get() - range.start.column.get() + 1)
    }
}

impl FromStr for WorksheetRange {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        if value.len() > 32 || value.is_empty() {
            return Err(invalid());
        }
        let Some((start, end)) = value.split_once(':') else {
            return Self::cells(value.parse()?);
        };
        if start.is_empty() || end.is_empty() {
            return Err(invalid());
        }
        let row = |s: &str| {
            s.strip_prefix('$')
                .unwrap_or(s)
                .bytes()
                .all(|c| c.is_ascii_digit())
        };
        let column = |s: &str| {
            s.strip_prefix('$')
                .unwrap_or(s)
                .bytes()
                .all(|c| c.is_ascii_alphabetic())
        };
        if row(start) && row(end) {
            let start: CellAddress = format!("A{start}").parse()?;
            let end: CellAddress = format!("A{end}").parse()?;
            return Self::rows(start.row, end.row);
        }
        if column(start) && column(end) {
            let start: CellAddress = format!("{start}1").parse()?;
            let end: CellAddress = format!("{end}1").parse()?;
            return Self::columns(start.column, end.column);
        }
        Self::cells(CellRange::new(start.parse()?, end.parse()?)?)
    }
}

impl fmt::Display for WorksheetRange {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Extent::Cells(range) => range.fmt(output),
            Extent::Rows(start, end) => write!(output, "{}:{}", start.get() + 1, end.get() + 1),
            Extent::Columns(start, end) => {
                crate::address::write_column_name(output, start.get())?;
                output.write_str(":")?;
                crate::address::write_column_name(output, end.get())
            }
        }
    }
}

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "Invalid worksheet-local range")
}
