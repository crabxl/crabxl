//! Canonical merged geometry and preserving source-model integration.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Merge a finite rectangle after validating the affected original graph.
    /// Covered values are removed; borders/protection use compact virtual styles.
    pub fn merge_cells(&mut self, id: SheetId, range: CellRange) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only merge editing is not implemented",
            ));
        }
        let source = self
            .sheets
            .iter()
            .position(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let plan = if self.sheets[source].original.is_some() {
            Some(self.editor.prepare_model(&self.sheets[source].name)?)
        } else {
            None
        };
        self.hyperlinks(id)?;
        let sheet = self.sheet(id)?;
        if sheet.merged_ranges().contains(range) {
            return Ok(());
        }
        crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
        self.prepare_style_edit()?;
        if let Some(plan) = &plan {
            self.reserve_workbook_patch(plan.bytes.max(self.editor.patch_bytes()))?;
        }
        let result = self.bank.merge_cells(id, range);
        if result.is_ok() {
            if let Some(plan) = plan {
                self.editor.commit_model(plan, id);
            } else {
                self.editor.created_values_dirty(id);
            }
            self.editor.styles_changed();
        }
        self.rebalance()?;
        result
    }

    /// Remove exact merged geometry while retaining the anchor value/appearance.
    pub fn unmerge_cells(&mut self, id: SheetId, range: CellRange) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.unmerge_cells(range))
    }

    pub(super) fn validate_normalized_styles(&self, id: SheetId) -> Result<()> {
        if self
            .sheets
            .iter()
            .any(|sheet| sheet.id == id && sheet.normalized_styles)
        {
            let catalog = self.bank.style_catalog().ok_or_else(|| {
                Error::new(ErrorKind::InvalidState, "Missing normalized merge styles")
            })?;
            self.editor.validate_style_edit(Some(catalog))?;
        }
        Ok(())
    }
    pub(super) fn export_normalized_styles(&mut self, id: SheetId) {
        if self
            .sheets
            .iter()
            .any(|sheet| sheet.id == id && sheet.normalized_styles)
        {
            self.editor.styles_changed();
        }
    }
    pub(super) fn commit_normalized_model(&mut self, plan: crate::editor::ModelPlan, id: SheetId) {
        self.editor.commit_model(plan, id);
        self.export_normalized_styles(id);
    }
}
