//! Lazy canonical hyperlink access under the shared source/model allowance.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Borrow canonical hyperlink declarations, resolving source targets once.
    /// A first typed request before materialization captures declarations during
    /// that cell stream. Ordinary scalar access keeps feature capture disabled.
    pub fn hyperlinks(&mut self, id: SheetId) -> Result<&crabxl_core::Hyperlinks> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if !self.sheets[index].loaded {
            self.sheets[index].hyperlinks_requested = true;
            self.sheet(id)?;
        }
        if !self.sheets[index].hyperlinks_loaded {
            self.rebalance()?;
            let maximum = self
                .allowance
                .retained_data_bytes
                .min(self.options.workbook.max_bytes);
            let available = maximum.saturating_sub(self.managed_retained_bytes());
            let links = self
                .editor
                .book
                .hyperlinks_with_allowance(self.sheets[index].name.as_ref(), available)?;
            self.bank.sheet_mut(id)?.adopt_hyperlinks(links)?;
            self.sheets[index].hyperlinks_loaded = true;
            self.rebalance()?;
        }
        Ok(self.bank.sheet(id)?.hyperlinks())
    }
    /// Replace one external/location point and preserve unrelated original XML.
    /// Empty anchors share the normal bounded value overlay; clearing keeps value.
    pub fn set_hyperlink(
        &mut self,
        id: SheetId,
        address: CellAddress,
        link: Option<crabxl_core::Hyperlink>,
    ) -> Result<()> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only hyperlink editing remains unimplemented",
            ));
        }
        if let Some(link) = &link {
            crate::hyperlinks::validate_link(address, link)?;
        }
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, "Unknown hyperlink sheet identity")
            })?;
        if self.sheets[index].original.is_none() {
            let result = self.bank.sheet_mut(id)?.set_hyperlink(address, link);
            if result.is_ok() {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let original = self.sheets[index].original.ok_or_else(|| {
            Error::new(ErrorKind::InvalidState, "Missing hyperlink source identity")
        })?;
        self.editor
            .prepare_hyperlink_patch(original, self.editor.patch_bytes())?;
        self.hyperlinks(id)?;
        let sheet = self.bank.sheet(id)?;
        let filler = link
            .as_ref()
            .filter(|_| {
                sheet
                    .get(address)
                    .is_none_or(|cell| matches!(cell.value, CellValue::Empty))
            })
            .map(crabxl_core::Hyperlink::initial_cell_value);
        let model_dirty = self.editor.model_is_dirty(&self.sheets[index].name);
        let value_plan = filler
            .as_ref()
            .map(|value| {
                self.editor
                    .prepare_value(&self.sheets[index].name, address, value)
            })
            .transpose()?;
        let base = value_plan
            .as_ref()
            .map_or(self.editor.patch_bytes(), |plan| plan.bytes);
        let planned = self.editor.prepare_hyperlink_patch(original, base)?;
        self.reserve_workbook_patch(planned)?;
        let result = self.bank.sheet_mut(id)?.set_hyperlink(address, link);
        if let Err(error) = result {
            self.rebalance()?;
            return Err(error);
        }
        if !model_dirty && let Some((plan, value)) = value_plan.zip(filler) {
            self.editor.commit_value(plan, value, true);
        }
        self.editor.commit_hyperlink_patch(original, id, planned);
        self.rebalance()
    }
}
