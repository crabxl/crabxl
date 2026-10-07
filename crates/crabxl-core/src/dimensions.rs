// SPDX-License-Identifier: MIT
// Selected row/column metadata layout adapted from umya-spreadsheet.
// Copyright (c) 2020 MathNya. Shared StyleId and bounded sparse vectors replace
// per-dimension boxed styles and upstream reader/writer ownership.
use crate::{ColumnIndex, Error, ErrorKind, Result, RowIndex, StyleId};

/// Explicit properties for one worksheet row, independent of its cells.
#[derive(Clone, Debug, PartialEq)]
pub struct RowDimension {
    /// Zero-based row identity.
    pub index: RowIndex,
    /// Optional point height, retaining absence independently of custom-height.
    pub height: Option<f64>,
    /// Workbook-local row format.
    pub style: Option<StyleId>,
    /// Explicit visibility override.
    pub hidden: Option<bool>,
    /// Outline level spelling represented as an unsigned integer.
    pub outline_level: Option<u32>,
    /// Collapsed outline marker.
    pub collapsed: Option<bool>,
    /// Explicit custom-height flag.
    pub custom_height: Option<bool>,
    /// Explicit custom-format flag.
    pub custom_format: Option<bool>,
    /// Thick upper edge.
    pub thick_top: Option<bool>,
    /// Thick lower edge.
    pub thick_bottom: Option<bool>,
    /// Optional extended row descent.
    pub descent: Option<f64>,
}
impl RowDimension {
    /// Construct a row record without installing application defaults.
    pub const fn new(index: RowIndex) -> Self {
        Self {
            index,
            height: None,
            style: None,
            hidden: None,
            outline_level: None,
            collapsed: None,
            custom_height: None,
            custom_format: None,
            thick_top: None,
            thick_bottom: None,
            descent: None,
        }
    }
    /// Validate finite numeric metadata without rejecting finite source literals.
    pub fn validate(&self) -> Result<()> {
        finite(self.height)?;
        finite(self.descent)
    }
}
/// Explicit properties for one inclusive worksheet column interval.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnDimension {
    /// First zero-based column.
    pub start: ColumnIndex,
    /// Last zero-based column.
    pub end: ColumnIndex,
    /// Optional character-unit width.
    pub width: Option<f64>,
    /// Workbook-local column format.
    pub style: Option<StyleId>,
    /// Explicit visibility override.
    pub hidden: Option<bool>,
    /// Best-fit marker, independent of width.
    pub best_fit: Option<bool>,
    /// Outline level.
    pub outline_level: Option<u32>,
    /// Collapsed outline marker.
    pub collapsed: Option<bool>,
    /// Explicit custom-width flag.
    pub custom_width: Option<bool>,
    /// Show phonetic text.
    pub phonetic: Option<bool>,
}
impl ColumnDimension {
    /// Construct an inclusive interval without installing width/style defaults.
    pub fn new(start: ColumnIndex, end: ColumnIndex) -> Result<Self> {
        if start > end {
            return Err(invalid("Reversed column dimension interval"));
        }
        Ok(Self {
            start,
            end,
            width: None,
            style: None,
            hidden: None,
            best_fit: None,
            outline_level: None,
            collapsed: None,
            custom_width: None,
            phonetic: None,
        })
    }
    /// Validate ordered geometry and finite numeric metadata.
    pub fn validate(&self) -> Result<()> {
        if self.start > self.end {
            return Err(invalid("Reversed column dimension interval"));
        }
        finite(self.width)
    }
}
/// Sparse canonical row/column metadata. Capacities participate in the owner cap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetDimensions {
    rows: Vec<RowDimension>,
    columns: Vec<ColumnDimension>,
}
impl SheetDimensions {
    /// Borrow sorted row records without expanding missing rows.
    pub fn rows(&self) -> &[RowDimension] {
        &self.rows
    }
    /// Borrow column intervals ordered by their first column.
    pub fn columns(&self) -> &[ColumnDimension] {
        &self.columns
    }
    /// Borrow an explicit row record.
    pub fn row(&self, index: RowIndex) -> Option<&RowDimension> {
        self.rows
            .binary_search_by_key(&index, |row| row.index)
            .ok()
            .map(|i| &self.rows[i])
    }
    /// Borrow an interval whose declaration starts at this column.
    pub fn column(&self, index: ColumnIndex) -> Option<&ColumnDimension> {
        self.columns
            .binary_search_by_key(&index, |column| column.start)
            .ok()
            .map(|i| &self.columns[i])
    }
    /// Actual retained vector capacities; the enclosing owner charges this header.
    pub fn heap_bytes(&self) -> usize {
        self.rows
            .capacity()
            .saturating_mul(size_of::<RowDimension>())
            .saturating_add(
                self.columns
                    .capacity()
                    .saturating_mul(size_of::<ColumnDimension>()),
            )
    }
    /// Validate all stored finite literals and sparse ordering.
    pub fn validate(&self) -> Result<()> {
        if self.rows.windows(2).any(|v| v[0].index >= v[1].index)
            || self.columns.windows(2).any(|v| v[0].start >= v[1].start)
        {
            return Err(invalid("Dimension declarations are not ordered"));
        }
        for row in &self.rows {
            row.validate()?;
        }
        for column in &self.columns {
            column.validate()?;
        }
        Ok(())
    }
    /// Replace/insert one row within the joint metadata allowance.
    pub fn set_row(&mut self, row: RowDimension, maximum: usize) -> Result<()> {
        row.validate()?;
        match self.rows.binary_search_by_key(&row.index, |v| v.index) {
            Ok(index) => {
                if self.heap_bytes() > maximum {
                    return Err(budget());
                }
                self.rows[index] = row;
            }
            Err(index) => {
                reserve(
                    &mut self.rows,
                    self.columns.capacity() * size_of::<ColumnDimension>(),
                    maximum,
                )?;
                self.rows.insert(index, row);
            }
        }
        Ok(())
    }
    /// Replace/insert one column interval within the joint metadata allowance.
    pub fn set_column(&mut self, column: ColumnDimension, maximum: usize) -> Result<()> {
        column.validate()?;
        match self
            .columns
            .binary_search_by_key(&column.start, |v| v.start)
        {
            Ok(index) => {
                if self.heap_bytes() > maximum {
                    return Err(budget());
                }
                self.columns[index] = column;
            }
            Err(index) => {
                reserve(
                    &mut self.columns,
                    self.rows.capacity() * size_of::<RowDimension>(),
                    maximum,
                )?;
                self.columns.insert(index, column);
            }
        }
        Ok(())
    }
    /// Group a bounded row interval with one reservation and an in-place merge.
    /// Existing height/style flags are retained; cells are not materialized.
    pub fn group_rows(
        &mut self,
        start: RowIndex,
        end: RowIndex,
        level: u32,
        hidden: bool,
        maximum: usize,
    ) -> Result<()> {
        if start > end {
            return Err(invalid("Reversed row group interval"));
        }
        let left = self.rows.partition_point(|row| row.index < start);
        let right = self.rows.partition_point(|row| row.index <= end);
        let count = (end.get() - start.get() + 1) as usize;
        let missing = count - (right - left);
        let old_len = self.rows.len();
        let length = old_len.saturating_add(missing);
        let other = self
            .columns
            .capacity()
            .saturating_mul(size_of::<ColumnDimension>());
        let capacity = self.rows.capacity().max(length);
        if other.saturating_add(capacity.saturating_mul(size_of::<RowDimension>())) > maximum {
            return Err(budget());
        }
        self.rows.try_reserve_exact(missing).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot reserve grouped row metadata",
                cause,
            )
        })?;
        if self.heap_bytes() > maximum {
            return Err(budget());
        }
        self.rows
            .resize_with(length, || RowDimension::new(RowIndex::FIRST));
        for source in (right..old_len).rev() {
            let row = std::mem::replace(&mut self.rows[source], RowDimension::new(RowIndex::FIRST));
            self.rows[source + missing] = row;
        }
        let mut existing = right;
        for offset in (0..count).rev() {
            let index = RowIndex::new(start.get() + offset as u32)?;
            let mut row = if existing > left && self.rows[existing - 1].index == index {
                existing -= 1;
                std::mem::replace(&mut self.rows[existing], RowDimension::new(RowIndex::FIRST))
            } else {
                RowDimension::new(index)
            };
            row.outline_level = Some(level);
            row.hidden = Some(hidden);
            self.rows[left + offset] = row;
        }
        Ok(())
    }
    /// Group columns as one declaration, retaining its first column's appearance.
    pub fn group_columns(
        &mut self,
        start: ColumnIndex,
        end: ColumnIndex,
        level: u32,
        hidden: bool,
        maximum: usize,
    ) -> Result<()> {
        let mut column = self
            .column(start)
            .cloned()
            .unwrap_or(ColumnDimension::new(start, end)?);
        column.end = end;
        if column.width.is_none() {
            column.width = Some(13.0);
            column.custom_width = Some(true);
        }
        column.outline_level = Some(level);
        column.hidden = Some(hidden);
        self.set_column(column, maximum)?;
        self.columns
            .retain(|column| column.start <= start || column.start > end);
        Ok(())
    }
    /// Remove an explicit row without changing other row identities.
    pub fn remove_row(&mut self, index: RowIndex) -> Option<RowDimension> {
        let position = self.rows.binary_search_by_key(&index, |v| v.index).ok()?;
        Some(self.rows.remove(position))
    }
    /// Remove an interval by its first column, preserving unrelated intervals.
    pub fn remove_column(&mut self, index: ColumnIndex) -> Option<ColumnDimension> {
        let position = self
            .columns
            .binary_search_by_key(&index, |v| v.start)
            .ok()?;
        Some(self.columns.remove(position))
    }
}
fn finite(value: Option<f64>) -> Result<()> {
    if value.is_some_and(|v| !v.is_finite()) {
        return Err(invalid("Non-finite dimension metadata"));
    }
    Ok(())
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Dimension metadata allowance exceeded",
    )
}
fn reserve<T>(values: &mut Vec<T>, other: usize, maximum: usize) -> Result<()> {
    let retained = other.saturating_add(values.capacity().saturating_mul(size_of::<T>()));
    if retained > maximum {
        return Err(budget());
    }
    if values.len() == values.capacity() {
        let geometric = values.capacity().max(4);
        let additional =
            if retained.saturating_add(geometric.saturating_mul(size_of::<T>())) <= maximum {
                geometric
            } else {
                1
            };
        if retained.saturating_add(additional.saturating_mul(size_of::<T>())) > maximum {
            return Err(budget());
        }
        values.try_reserve_exact(additional).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot reserve dimension metadata",
                cause,
            )
        })?;
        if other.saturating_add(values.capacity().saturating_mul(size_of::<T>())) > maximum {
            return Err(budget());
        }
    }
    Ok(())
}
