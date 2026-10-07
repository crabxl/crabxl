//! Structure operations for the canonical Worksheet owner.
use super::*;

impl Worksheet {
    /// Insert empty rows; out-of-bounds movement is rejected before mutation.
    pub fn insert_rows(&mut self, at: RowIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, true, true)
    }
    /// Delete rows and shift subsequent physical cells upward.
    pub fn delete_rows(&mut self, at: RowIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, true, false)
    }
    /// Insert empty columns; rows/columns are never densely expanded.
    pub fn insert_columns(&mut self, at: crate::ColumnIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, false, true)
    }
    /// Delete columns and shift subsequent physical cells leftward.
    pub fn delete_columns(&mut self, at: crate::ColumnIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, false, false)
    }
    /// Move present cells by signed offsets; overlapping destinations are
    /// overwritten after all source cells are removed. Formulas stay verbatim.
    pub fn move_range(&mut self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        self.validate_range(range, rows, columns)?;
        if rows == 0 && columns == 0 {
            return Ok(());
        }
        let destination = CellRange::new(
            offset(range.start, rows, columns)?,
            offset(range.end, rows, columns)?,
        )?;
        self.guard_merged_structure(range, destination)?;
        self.work_allowance(0)?;
        let original = std::mem::take(&mut self.cells);
        let mut moved = BTreeMap::new();
        for (_, mut cell) in original {
            if range.contains(cell.address) {
                cell.address = offset(cell.address, rows, columns)?;
                moved.insert(key(cell.address), cell);
            } else if !destination.contains(cell.address) {
                self.cells.insert(key(cell.address), cell);
            }
        }
        for (key, cell) in moved {
            self.append_cursor = self.append_cursor.max(cell.address.row.get() + 1);
            self.cells.insert(key, cell);
        }
        self.recount();
        self.dirty = true;
        Ok(())
    }
    /// Move a range while translating relative references in its normal formulas.
    /// Validates every translation and retained/transient allowance before any
    /// cell moves. New formulas discard old caches; other values/styles move as-is.
    /// References outside the moved cells are not updated automatically.
    pub fn move_range_translated(
        &mut self,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.validate_range(range, rows, columns)?;
        if rows == 0 && columns == 0 {
            return Ok(());
        }
        self.guard_merged_structure(
            range,
            CellRange::new(
                offset(range.start, rows, columns)?,
                offset(range.end, rows, columns)?,
            )?,
        )?;
        let formulas = self
            .cells
            .values()
            .filter(|cell| {
                range.contains(cell.address) && matches!(cell.value, CellValue::Formula(_))
            })
            .count();
        let mut translated = Vec::new();
        self.work_allowance(
            formulas.saturating_mul(size_of::<(CellAddress, Box<crate::Formula>)>()),
        )?;
        translated.try_reserve_exact(formulas).map_err(|error| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate formula move staging",
                error,
            )
        })?;
        let base = self
            .charged
            .saturating_add(self.cell_work_bytes())
            .saturating_add(
                translated
                    .capacity()
                    .saturating_mul(size_of::<(CellAddress, Box<crate::Formula>)>()),
            );
        let mut staged = 0usize;
        let mut retained = self.charged;
        for cell in self
            .cells
            .values()
            .filter(|cell| range.contains(cell.address))
        {
            if let CellValue::Formula(formula) = &cell.value {
                if matches!(
                    formula.formula_type(),
                    crate::FormulaType::Array | crate::FormulaType::DataTable
                ) {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Structured formula graph translation is not implemented",
                    )
                    .with_cell(cell.address));
                }
                let maximum = self
                    .limits
                    .max_bytes
                    .saturating_sub(base)
                    .saturating_sub(staged)
                    .saturating_sub(size_of::<crate::Formula>());
                let expression = crate::translate_expression(
                    formula.expression(),
                    i64::from(rows),
                    i64::from(columns),
                    maximum,
                )
                .map_err(|error| error.with_cell(cell.address))?;
                let value = crate::Formula::from_source(expression.into_boxed_str(), None, None)?;
                staged = staged.saturating_add(value.memory_bytes());
                retained = retained
                    .saturating_sub(formula.memory_bytes())
                    .saturating_add(value.memory_bytes());
                self.check(base.saturating_add(staged), self.len())?;
                self.check(retained, self.len())?;
                translated.push((offset(cell.address, rows, columns)?, Box::new(value)));
            }
        }
        self.move_range(range, rows, columns)?;
        // Each staged address belongs to a physical source cell already moved
        // by the validated operation; no further input/limit checks are needed.
        for (address, formula) in translated {
            if let Some(cell) = self.cells.get_mut(&key(address)) {
                cell.value = CellValue::Formula(formula);
            }
        }
        self.recount();
        Ok(())
    }
    /// Copy physical cells to an offset rectangle, retaining styles and formulas.
    /// Clones only selected payloads and checks transient work before mutation.
    pub fn copy_range(&mut self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        self.validate_range(range, rows, columns)?;
        if rows == 0 && columns == 0 {
            return Ok(());
        }
        let destination = CellRange::new(
            offset(range.start, rows, columns)?,
            offset(range.end, rows, columns)?,
        )?;
        self.guard_merged_structure(range, destination)?;
        let selected = self
            .cells
            .values()
            .filter(|cell| range.contains(cell.address));
        let extra = selected
            .clone()
            .map(|cell| ENTRY_BYTES.saturating_add(charge(cell)))
            .fold(0usize, usize::saturating_add);
        let count = selected.clone().count();
        let removed = self
            .cells
            .values()
            .filter(|cell| destination.contains(cell.address));
        let removed_bytes = removed.clone().map(charge).sum::<usize>();
        let removed_count = removed.count();
        self.check(
            self.charged
                .saturating_sub(removed_bytes)
                .saturating_add(extra),
            self.len() - removed_count + count,
        )?;
        // Includes clone Vec and destination nodes conservatively.
        self.work_allowance(extra)?;
        let mut copies = Vec::new();
        copies.try_reserve_exact(count).map_err(|error| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate copy staging",
                error,
            )
        })?;
        for cell in selected {
            let mut cell = cell.clone();
            cell.address = offset(cell.address, rows, columns)?;
            copies.push(cell);
        }
        self.cells
            .retain(|_, cell| !destination.contains(cell.address));
        for cell in copies {
            self.append_cursor = self.append_cursor.max(cell.address.row.get() + 1);
            self.cells.insert(key(cell.address), cell);
        }
        self.recount();
        self.dirty = true;
        Ok(())
    }
    pub(super) fn validate_range(&self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        offset(range.start, rows, columns)?;
        offset(range.end, rows, columns)?;
        Ok(())
    }
    pub(super) fn shift(&mut self, at: u32, count: u32, rows: bool, insert: bool) -> Result<()> {
        let maximum = if rows { MAX_ROWS } else { MAX_COLUMNS };
        if count == 0 || at.checked_add(count).is_none_or(|end| end > maximum) {
            return Err(invalid("Invalid structural edit count"));
        }
        if self.merges.ranges().iter().any(|merge| {
            if rows {
                merge.range().end.row.get() >= at
            } else {
                merge.range().end.column.get() >= at
            }
        }) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Affected merged geometry structural edits are not implemented",
            ));
        }
        if insert {
            for cell in self.cells.values() {
                let value = if rows {
                    cell.address.row.get()
                } else {
                    cell.address.column.get()
                };
                if value >= at
                    && value
                        .checked_add(count)
                        .is_none_or(|index| index >= maximum)
                {
                    return Err(invalid("Structural edit exceeds worksheet bounds"));
                }
            }
            if rows
                && self.append_cursor > at
                && self
                    .append_cursor
                    .checked_add(count)
                    .is_none_or(|end| end > MAX_ROWS)
            {
                return Err(invalid("Append extent exceeds row bounds"));
            }
        }
        self.work_allowance(0)?;
        for (_, mut cell) in std::mem::take(&mut self.cells) {
            let value = if rows {
                cell.address.row.get()
            } else {
                cell.address.column.get()
            };
            if !insert && (at..at + count).contains(&value) {
                continue;
            }
            let new = if insert && value >= at {
                value + count
            } else if !insert && value >= at + count {
                value - count
            } else {
                value
            };
            cell.address = if rows {
                CellAddress::new(new, cell.address.column.get())?
            } else {
                CellAddress::new(cell.address.row.get(), new)?
            };
            self.cells.insert(key(cell.address), cell);
        }
        if rows && self.append_cursor > at {
            self.append_cursor = if insert {
                self.append_cursor + count
            } else {
                self.append_cursor - count.min(self.append_cursor - at)
            };
        }
        self.recount();
        self.dirty = true;
        Ok(())
    }
}
