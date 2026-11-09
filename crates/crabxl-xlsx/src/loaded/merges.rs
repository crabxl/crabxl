//! Canonical merged geometry and preserving source-model integration.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Normalize a merge anchor's border without registering merge geometry.
    pub fn prepare_merge_anchor(&mut self, id: SheetId, range: CellRange) -> Result<()> {
        self.prepare_style_edit()?;
        let sheet = self.sheet(id)?;
        let anchor = sheet.style_at(range.start);
        let corner = sheet
            .get(range.end)
            .is_some()
            .then(|| sheet.style_at(range.end));
        self.rebalance()?;
        let result = self.bank.prepare_merge(range, anchor, corner);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        self.assign_style(id, range.start, result?.1, false)
    }

    /// Merge a finite rectangle after validating the affected original graph.
    /// Covered values are removed; borders/protection use compact virtual styles.
    pub fn merge_cells(&mut self, id: SheetId, range: CellRange) -> Result<()> {
        self.apply_merged_cells(id, range, true)
    }
    /// Realize merged appearance independently of worksheet merge declarations.
    pub fn format_merged_cells(&mut self, id: SheetId, range: CellRange) -> Result<()> {
        self.apply_merged_cells(id, range, false)
    }
    fn apply_merged_cells(&mut self, id: SheetId, range: CellRange, declare: bool) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        let source = self
            .sheets
            .iter()
            .position(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let mut plan = if self.sheets[source].original.is_some() {
            Some(self.editor.prepare_model(&self.sheets[source].name)?)
        } else {
            None
        };
        self.hyperlinks(id)?;
        let sheet = self.sheet(id)?;
        if declare && sheet.merged_ranges().contains(range) {
            return Ok(());
        }
        let hyperlink_patch = if !sheet.hyperlinks().is_empty() {
            if let (Some(original), Some(plan)) = (self.sheets[source].original, plan.as_mut()) {
                let bytes = self.editor.prepare_hyperlink_patch(original, plan.bytes)?;
                plan.bytes = bytes;
                Some((original, bytes))
            } else {
                None
            }
        } else {
            None
        };
        crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
        self.prepare_style_edit()?;
        if let Some(plan) = &plan {
            self.reserve_workbook_patch(plan.bytes.max(self.editor.patch_bytes()))?;
        }
        let result = if declare {
            self.bank.merge_cells(id, range)
        } else {
            self.bank.format_merged_cells(id, range)
        };
        if result.is_ok() {
            if let Some(plan) = plan {
                self.editor.commit_model(plan, id);
            } else {
                self.editor.created_values_dirty(id);
            }
            self.editor.styles_changed();
            if let Some((original, bytes)) = hyperlink_patch {
                self.editor.commit_hyperlink_patch(original, id, bytes);
            }
        }
        self.rebalance()?;
        result
    }

    /// Remove exact merged geometry while retaining the anchor value/appearance.
    pub fn unmerge_cells(&mut self, id: SheetId, range: CellRange) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.unmerge_cells(range))
    }

    /// Add serialization-only merge metadata through the preserving coordinator.
    pub fn replace_merge_declarations(&mut self, id: SheetId, ranges: &[CellRange]) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.replace_merge_declarations(ranges))
    }
    /// Add serialization-only merge metadata through the preserving coordinator.
    pub fn add_merge_declaration(&mut self, id: SheetId, range: CellRange) -> Result<bool> {
        self.edit_structure_when(
            id,
            |sheet| sheet.add_merge_declaration(range),
            |changed| *changed,
        )
    }
    /// Add raw set membership without suppressing contained declarations.
    pub fn add_exact_merge_declaration(&mut self, id: SheetId, range: CellRange) -> Result<bool> {
        self.edit_structure_when(
            id,
            |sheet| sheet.add_exact_merge_declaration(range),
            |changed| *changed,
        )
    }
    /// Remove raw merge metadata, retaining the already realized cell model.
    pub fn remove_merge_declaration(&mut self, id: SheetId, range: CellRange) -> Result<bool> {
        self.edit_structure_when(
            id,
            |sheet| sheet.remove_merge_declaration(range),
            |changed| *changed,
        )
    }
    /// Replace live declaration coordinates without moving existing cell contents.
    pub fn replace_merge_declaration(
        &mut self,
        id: SheetId,
        identity: u64,
        range: CellRange,
    ) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.replace_merge_declaration(identity, range))
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
