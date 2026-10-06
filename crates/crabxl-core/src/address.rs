// SPDX-License-Identifier: MIT
// Coordinate scanning adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

use crate::{Error, ErrorKind, Result};
use std::{fmt, str::FromStr};

/// Maximum worksheet rows in XLSX.
pub const MAX_ROWS: u32 = 1_048_576;
/// Maximum worksheet columns in XLSX.
pub const MAX_COLUMNS: u32 = 16_384;

/// A validated zero-based worksheet row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowIndex(u32);
impl RowIndex {
    /// First legal worksheet row.
    pub const FIRST: Self = Self(0);
    /// Last legal worksheet row.
    pub const LAST: Self = Self(MAX_ROWS - 1);
    /// Validate a zero-based row index.
    pub fn new(value: u32) -> Result<Self> {
        if value < MAX_ROWS {
            Ok(Self(value))
        } else {
            Err(invalid_address())
        }
    }
    /// Return the zero-based index.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A validated zero-based worksheet column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnIndex(u32);
impl ColumnIndex {
    /// First legal worksheet column.
    pub const FIRST: Self = Self(0);
    /// Last legal worksheet column.
    pub const LAST: Self = Self(MAX_COLUMNS - 1);
    /// Validate a zero-based column index.
    pub fn new(value: u32) -> Result<Self> {
        if value < MAX_COLUMNS {
            Ok(Self(value))
        } else {
            Err(invalid_address())
        }
    }
    /// Return the zero-based index.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A validated cell position, independent of any workbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellAddress {
    /// Zero-based row.
    pub row: RowIndex,
    /// Zero-based column.
    pub column: ColumnIndex,
}
impl CellAddress {
    /// Construct a cell address from zero-based indices.
    pub fn new(row: u32, column: u32) -> Result<Self> {
        Ok(Self {
            row: RowIndex::new(row)?,
            column: ColumnIndex::new(column)?,
        })
    }
}

fn invalid_address() -> Error {
    Error::new(
        ErrorKind::InvalidData,
        "Invalid or out-of-bounds XLSX cell address",
    )
}

impl FromStr for CellAddress {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        let bytes = value.as_bytes();
        let mut offset = usize::from(bytes.first() == Some(&b'$'));
        let mut column = 0u32;
        while let Some(&letter) = bytes.get(offset) {
            if !letter.is_ascii_alphabetic() {
                break;
            }
            column = column
                .checked_mul(26)
                .and_then(|v| v.checked_add(u32::from(letter.to_ascii_uppercase() - b'A' + 1)))
                .ok_or_else(invalid_address)?;
            offset += 1;
        }
        if bytes.get(offset) == Some(&b'$') {
            offset += 1;
        }
        let row_start = offset;
        let mut row = 0u32;
        while let Some(&digit) = bytes.get(offset) {
            if !digit.is_ascii_digit() {
                return Err(invalid_address());
            }
            row = row
                .checked_mul(10)
                .and_then(|v| v.checked_add(u32::from(digit - b'0')))
                .ok_or_else(invalid_address)?;
            offset += 1;
        }
        if offset == row_start || bytes.get(row_start) == Some(&b'0') {
            return Err(invalid_address());
        }
        Self::new(
            row.checked_sub(1).ok_or_else(invalid_address)?,
            column.checked_sub(1).ok_or_else(invalid_address)?,
        )
    }
}

impl fmt::Display for CellAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_column_name(f, self.column.get())?;
        write!(f, "{}", self.row.get() + 1)
    }
}

// Shared by physical addresses and formula references (which can extend beyond
// the worksheet's physical column boundary during baseline translation).
pub(crate) fn write_column_name(output: &mut impl fmt::Write, column: u32) -> fmt::Result {
    let mut column = column.checked_add(1).ok_or(fmt::Error)?;
    let mut letters = [0u8; 8];
    let mut start = letters.len();
    while column > 0 {
        start -= 1;
        letters[start] = b'A' + ((column - 1) % 26) as u8;
        column = (column - 1) / 26;
    }
    for &letter in &letters[start..] {
        output.write_char(char::from(letter))?;
    }
    Ok(())
}
