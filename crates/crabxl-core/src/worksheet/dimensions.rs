//! Dimensions operations for the canonical Worksheet owner.
use super::*;

impl Worksheet {
    /// Borrow sparse dimension metadata without expanding cells.
    pub fn dimensions(&self) -> &crate::SheetDimensions {
        &self.dimensions
    }
    /// Replace all dimension declarations within the retained-data allowance.
    pub fn set_dimensions(&mut self, dimensions: crate::SheetDimensions) -> Result<()> {
        dimensions.validate()?;
        let charged = self
            .charged
            .saturating_sub(self.dimensions.heap_bytes())
            .saturating_add(dimensions.heap_bytes());
        self.check(charged, self.len())?;
        self.dimensions = dimensions;
        self.charged = charged;
        self.dirty = true;
        Ok(())
    }
    /// Update one row without copying unrelated dimension vectors or cells.
    pub fn set_row_dimension(&mut self, row: crate::RowDimension) -> Result<()> {
        let other = self.charged.saturating_sub(self.dimensions.heap_bytes());
        let result = self
            .dimensions
            .set_row(row, self.limits.max_bytes.saturating_sub(other));
        self.charged = other.saturating_add(self.dimensions.heap_bytes());
        if result.is_ok() {
            self.dirty = true;
        }
        result
    }
    /// Update one column interval under the same aggregate retained allowance.
    pub fn set_column_dimension(&mut self, column: crate::ColumnDimension) -> Result<()> {
        let other = self.charged.saturating_sub(self.dimensions.heap_bytes());
        let result = self
            .dimensions
            .set_column(column, self.limits.max_bytes.saturating_sub(other));
        self.charged = other.saturating_add(self.dimensions.heap_bytes());
        if result.is_ok() {
            self.dirty = true;
        }
        result
    }
    /// Remove one explicit row declaration, retaining reusable capacity.
    pub fn remove_row_dimension(&mut self, index: RowIndex) -> Option<crate::RowDimension> {
        let dimension = self.dimensions.remove_row(index);
        if dimension.is_some() {
            self.dirty = true;
        }
        dimension
    }
    /// Remove one column declaration by its first column.
    pub fn remove_column_dimension(
        &mut self,
        index: ColumnIndex,
    ) -> Option<crate::ColumnDimension> {
        let dimension = self.dimensions.remove_column(index);
        if dimension.is_some() {
            self.dirty = true;
        }
        dimension
    }
    /// Group rows without cloning cell or metadata vectors.
    pub fn group_rows(
        &mut self,
        start: RowIndex,
        end: RowIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        let other = self.charged.saturating_sub(self.dimensions.heap_bytes());
        let result = self.dimensions.group_rows(
            start,
            end,
            level,
            hidden,
            self.limits.max_bytes.saturating_sub(other),
        );
        self.charged = other.saturating_add(self.dimensions.heap_bytes());
        if result.is_ok() {
            self.dirty = true;
        }
        result
    }
    /// Group one column interval while preserving its first appearance.
    pub fn group_columns(
        &mut self,
        start: ColumnIndex,
        end: ColumnIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        let other = self.charged.saturating_sub(self.dimensions.heap_bytes());
        let result = self.dimensions.group_columns(
            start,
            end,
            level,
            hidden,
            self.limits.max_bytes.saturating_sub(other),
        );
        self.charged = other.saturating_add(self.dimensions.heap_bytes());
        if result.is_ok() {
            self.dirty = true;
        }
        result
    }
}
