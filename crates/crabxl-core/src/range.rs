// SPDX-License-Identifier: MIT
// Axis-optional range layout adapted from umya-spreadsheet.
// Copyright (c) 2020 MathNya. Provenance: third_party/ports.json.
//! Allocation-free geometry for finite rectangles and complete worksheet axes.

use crate::{CellAddress, CellRange, ColumnIndex, Error, ErrorKind, Result, RowIndex};
use std::{fmt, str::FromStr};

impl CellRange {
    /// Smallest rectangle containing both inputs; this is a bounding union.
    pub fn union(self, other: Self) -> Self {
        Self {
            start: CellAddress {
                row: self.start.row.min(other.start.row),
                column: self.start.column.min(other.start.column),
            },
            end: CellAddress {
                row: self.end.row.max(other.end.row),
                column: self.end.column.max(other.end.column),
            },
        }
    }
    /// Whether another finite rectangle is entirely covered.
    pub fn contains_range(self, other: Self) -> bool {
        self.contains(other.start) && self.contains(other.end)
    }
    /// Adjust each edge independently, validating overflow and worksheet bounds.
    /// Positive deltas move an edge right or down; reversed results are rejected.
    pub fn adjusted(self, left: i64, top: i64, right: i64, bottom: i64) -> Result<Self> {
        fn coordinate(value: u32, delta: i64) -> Result<u32> {
            i64::from(value)
                .checked_add(delta)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "Adjusted range exceeds coordinate bounds",
                    )
                })
        }
        Self::new(
            CellAddress::new(
                coordinate(self.start.row.get(), top)?,
                coordinate(self.start.column.get(), left)?,
            )?,
            CellAddress::new(
                coordinate(self.end.row.get(), bottom)?,
                coordinate(self.end.column.get(), right)?,
            )?,
        )
    }
    /// Intersection of two finite rectangles without allocating coordinates.
    pub fn intersection(self, other: Self) -> Option<Self> {
        self.intersects(other).then(|| Self {
            start: CellAddress {
                row: self.start.row.max(other.start.row),
                column: self.start.column.max(other.start.column),
            },
            end: CellAddress {
                row: self.end.row.min(other.end.row),
                column: self.end.column.min(other.end.column),
            },
        })
    }
    /// Disjoint rectangles remaining after removing another rectangle.
    /// At most four pieces are returned, even for a whole worksheet range.
    pub fn difference(self, other: Self) -> [Option<Self>; 4] {
        let Some(overlap) = self.intersection(other) else {
            return [Some(self), None, None, None];
        };
        let mut pieces = [None; 4];
        if self.start.row < overlap.start.row {
            pieces[0] = RowIndex::new(overlap.start.row.get() - 1)
                .ok()
                .map(|row| Self {
                    start: self.start,
                    end: CellAddress {
                        row,
                        column: self.end.column,
                    },
                });
        }
        if overlap.end.row < self.end.row {
            pieces[1] = RowIndex::new(overlap.end.row.get() + 1)
                .ok()
                .map(|row| Self {
                    start: CellAddress {
                        row,
                        column: self.start.column,
                    },
                    end: self.end,
                });
        }
        if self.start.column < overlap.start.column {
            pieces[2] = ColumnIndex::new(overlap.start.column.get() - 1)
                .ok()
                .map(|column| Self {
                    start: CellAddress {
                        row: overlap.start.row,
                        column: self.start.column,
                    },
                    end: CellAddress {
                        row: overlap.end.row,
                        column,
                    },
                });
        }
        if overlap.end.column < self.end.column {
            pieces[3] = ColumnIndex::new(overlap.end.column.get() + 1)
                .ok()
                .map(|column| Self {
                    start: CellAddress {
                        row: overlap.start.row,
                        column,
                    },
                    end: CellAddress {
                        row: overlap.end.row,
                        column: self.end.column,
                    },
                });
        }
        pieces
    }
}

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
