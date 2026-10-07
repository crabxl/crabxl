//! Dimensions operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Update canonical row metadata after validating the affected source graph.
    pub fn set_row_dimension(
        &mut self,
        id: SheetId,
        dimension: crabxl_core::RowDimension,
    ) -> Result<()> {
        dimension.validate()?;
        if dimension.descent.is_some() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Extended row descent serialization is not implemented",
            ));
        }
        self.edit_dimension(id, |sheet| sheet.set_row_dimension(dimension))
    }
    /// Update canonical column metadata after validating the affected source graph.
    pub fn set_column_dimension(
        &mut self,
        id: SheetId,
        dimension: crabxl_core::ColumnDimension,
    ) -> Result<()> {
        dimension.validate()?;
        self.edit_dimension(id, |sheet| sheet.set_column_dimension(dimension))
    }
    /// Remove row metadata through the preserving canonical coordinator.
    pub fn remove_row_dimension(&mut self, id: SheetId, index: RowIndex) -> Result<bool> {
        let existed = self.sheet(id)?.dimensions().row(index).is_some();
        if existed {
            self.edit_dimension(id, |sheet| {
                sheet.remove_row_dimension(index);
                Ok(())
            })?;
        }
        Ok(existed)
    }
    /// Remove column metadata through the preserving canonical coordinator.
    pub fn remove_column_dimension(
        &mut self,
        id: SheetId,
        index: crabxl_core::ColumnIndex,
    ) -> Result<bool> {
        let existed = self.sheet(id)?.dimensions().column(index).is_some();
        if existed {
            self.edit_dimension(id, |sheet| {
                sheet.remove_column_dimension(index);
                Ok(())
            })?;
        }
        Ok(existed)
    }
    /// Group row metadata without repeatedly scanning cells or source XML.
    pub fn group_rows(
        &mut self,
        id: SheetId,
        start: RowIndex,
        end: RowIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        self.edit_dimension(id, |sheet| sheet.group_rows(start, end, level, hidden))
    }
    /// Group a column interval through the preserving canonical coordinator.
    pub fn group_columns(
        &mut self,
        id: SheetId,
        start: crabxl_core::ColumnIndex,
        end: crabxl_core::ColumnIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        self.edit_dimension(id, |sheet| sheet.group_columns(start, end, level, hidden))
    }
    pub(super) fn edit_dimension(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut crabxl_core::WorksheetEditor<'_>) -> Result<()>,
    ) -> Result<()> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only dimension editing remains unimplemented",
            ));
        }
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if source.original.is_none() || self.editor.model_is_dirty(&source.name) {
            let created = source.original.is_none();
            let result = edit(&mut self.bank.sheet_mut(id)?);
            if result.is_ok() && created {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        self.edit_structure_when(
            id,
            |sheet| {
                edit(sheet)?;
                Ok(true)
            },
            |changed| *changed,
        )?;
        Ok(())
    }
}
