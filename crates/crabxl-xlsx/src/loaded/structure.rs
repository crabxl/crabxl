//! Structure operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Remove one physical cell from a supported source-backed model and
    /// transfer its owned value/style to the caller. Logical append extent stays.
    /// Affected unmodeled graphs reject before any cell or package mutation.
    pub fn remove_cell(&mut self, id: SheetId, address: CellAddress) -> Result<Option<Cell>> {
        self.edit_structure_when(id, |sheet| Ok(sheet.remove(address)), Option::is_some)
    }
    /// Insert rows in a supported source-backed cell model. Unmodeled affected
    /// worksheet graphs are rejected before mutation; formulas are not translated.
    pub fn insert_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.insert_rows(at, count))
    }
    /// Delete rows while retaining unrelated original package parts.
    pub fn delete_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.delete_rows(at, count))
    }
    /// Insert columns without cloning the whole canonical worksheet.
    pub fn insert_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.insert_columns(at, count))
    }
    /// Delete columns using the same guarded source/model coordinator.
    pub fn delete_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.delete_columns(at, count))
    }
    /// Move a source-backed rectangle; reference expressions remain unchanged.
    pub fn move_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.move_range(range, rows, columns))
    }
    /// Move and translate relative references inside moved normal formulas.
    pub fn move_range_translated(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure(id, |sheet| {
            sheet.move_range_translated(range, rows, columns)
        })
    }
    /// Copy an actual rectangle under aggregate cell/payload allowances.
    pub fn copy_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.copy_range(range, rows, columns))
    }
    pub(super) fn edit_structure(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<()>,
    ) -> Result<()> {
        self.edit_structure_when(id, edit, |_| true)
    }
    pub(super) fn edit_structure_when<T>(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<T>,
        changed: impl FnOnce(&T) -> bool,
    ) -> Result<T> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only structural editing remains unimplemented",
            ));
        }
        let index = self
            .sheets
            .iter()
            .position(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].original.is_none() {
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
            let result = edit(&mut self.bank.sheet_mut(id)?);
            if let Ok(value) = &result
                && changed(value)
            {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let plan = self.editor.prepare_model(&self.sheets[index].name)?;
        self.sheet(id)?;
        crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
        self.validate_normalized_styles(id)?;
        self.reserve_workbook_patch(plan.bytes.max(self.editor.patch_bytes()))?;
        let result = edit(&mut self.bank.sheet_mut(id)?);
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                self.rebalance()?;
                return Err(error);
            }
        };
        if changed(&value) {
            self.commit_normalized_model(plan, id);
        }
        self.rebalance()?;
        Ok(value)
    }
}
