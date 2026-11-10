//! Structure operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Remove one physical cell from a supported source-backed model and
    /// transfer its owned value/style to the caller. Logical append extent stays.
    /// Affected unmodeled graphs reject before any cell or package mutation.
    pub fn remove_cell(&mut self, id: SheetId, address: CellAddress) -> Result<Option<Cell>> {
        self.remove_cell_with_hyperlink(id, address)
            .map(|(cell, _)| cell)
    }
    /// Remove a point and transfer its metadata after the original graph preflight.
    pub fn remove_cell_with_hyperlink(
        &mut self,
        id: SheetId,
        address: CellAddress,
    ) -> Result<(Option<Cell>, Option<crabxl_core::Hyperlink>)> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let adopted = self.sheets[index].features.is_none();
        let result = (|| {
            let mut comments = self.sheet_features(id)?.comments.clone();
            comments.retain(|comment| comment.address != address);
            let result = self
                .edit_structure_when(
                    id,
                    |sheet| {
                        let removed = sheet.remove_with_hyperlink(address)?;
                        let changed = removed.0.is_some() || removed.1.is_some();
                        Ok((removed, changed))
                    },
                    |(_, changed)| *changed,
                )
                .map(|(cell, _)| cell)?;
            if let Some(source) = self.sheets.iter_mut().find(|source| source.id == id)
                && let Some(state) = &mut source.features
            {
                state.current.comments = comments;
            }
            Ok(result)
        })();
        if result.is_err() && adopted {
            self.sheets[index].features = None;
            self.rebalance()?;
        }
        result
    }
    /// Insert rows in a supported source-backed cell model. Unmodeled affected
    /// worksheet graphs are rejected before mutation; formulas are not translated.
    pub fn insert_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Shift {
                at: at.get(),
                count,
                rows: true,
                insert: true,
            },
            |sheet| sheet.insert_rows(at, count),
        )
    }
    /// Delete rows while retaining unrelated original package parts.
    pub fn delete_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Shift {
                at: at.get(),
                count,
                rows: true,
                insert: false,
            },
            |sheet| sheet.delete_rows(at, count),
        )
    }
    /// Insert columns without cloning the whole canonical worksheet.
    pub fn insert_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Shift {
                at: at.get(),
                count,
                rows: false,
                insert: true,
            },
            |sheet| sheet.insert_columns(at, count),
        )
    }
    /// Delete columns using the same guarded source/model coordinator.
    pub fn delete_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Shift {
                at: at.get(),
                count,
                rows: false,
                insert: false,
            },
            |sheet| sheet.delete_columns(at, count),
        )
    }
    /// Move a source-backed rectangle; reference expressions remain unchanged.
    pub fn move_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Range {
                range,
                rows,
                columns,
                copy: false,
            },
            |sheet| sheet.move_range(range, rows, columns),
        )
    }
    /// Move and translate relative references inside moved normal formulas.
    pub fn move_range_translated(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Range {
                range,
                rows,
                columns,
                copy: false,
            },
            |sheet| sheet.move_range_translated(range, rows, columns),
        )
    }
    /// Copy an actual rectangle under aggregate cell/payload allowances.
    pub fn copy_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Range {
                range,
                rows,
                columns,
                copy: true,
            },
            |sheet| sheet.copy_range(range, rows, columns),
        )
    }
    fn edit_structure_with_comments(
        &mut self,
        id: SheetId,
        edit_comments: crabxl_core::features::CommentEdit,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<()>,
    ) -> Result<()> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let adopted = self.sheets[index].features.is_none();
        let staged = (|| {
            let comments = edit_comments.apply(&self.sheet_features(id)?.comments)?;
            let scratch = comments
                .iter()
                .map(|comment| size_of::<crabxl_core::features::Comment>() + comment.heap_bytes())
                .sum();
            self.edit_structure_with_work(id, edit, |_| true, scratch)?;
            Ok::<_, Error>(comments)
        })();
        let comments = match staged {
            Ok(comments) => comments,
            Err(error) => {
                if adopted {
                    self.sheets[index].features = None;
                    self.rebalance()?;
                }
                return Err(error);
            }
        };
        if let Some(source) = self.sheets.iter_mut().find(|source| source.id == id)
            && let Some(state) = &mut source.features
        {
            state.current.comments = comments;
        }
        Ok(())
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
        self.edit_structure_with_work(id, edit, changed, 0)
    }
    fn edit_structure_with_work<T>(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<T>,
        changed: impl FnOnce(&T) -> bool,
        scratch: usize,
    ) -> Result<T> {
        let index = self
            .sheets
            .iter()
            .position(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].kind != crate::SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Chartsheets do not contain editable cells",
            ));
        }
        if self.sheets[index].original.is_none() {
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
            self.reserve_workbook_patch(self.editor.patch_bytes().saturating_add(scratch))?;
            let result = edit(&mut self.bank.sheet_mut(id)?);
            if let Ok(value) = &result
                && changed(value)
            {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let original = self.sheets[index].original.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidState,
                "Missing structural source identity",
            )
        })?;
        // A committed model was validated when it was committed, and later edits
        // go through checked model APIs, so a full pass per edit is not needed.
        let validated = self.editor.model_is_dirty(&self.sheets[index].name);
        let mut plan = self.editor.prepare_model(&self.sheets[index].name)?;
        self.hyperlinks(id)?;
        let had_hyperlinks = !self.bank.sheet(id)?.hyperlinks().is_empty();
        let hyperlink_bytes = if had_hyperlinks {
            Some(self.editor.prepare_hyperlink_patch(original, plan.bytes)?)
        } else {
            None
        };
        if let Some(bytes) = hyperlink_bytes {
            plan.bytes = bytes;
        }
        if !validated {
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
        }
        self.validate_normalized_styles(id)?;
        self.reserve_workbook_patch(
            plan.bytes
                .max(self.editor.patch_bytes())
                .saturating_add(scratch),
        )?;
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
            if let Some(bytes) = hyperlink_bytes {
                self.editor.commit_hyperlink_patch(original, id, bytes);
            }
        }
        self.rebalance()?;
        Ok(value)
    }
}
