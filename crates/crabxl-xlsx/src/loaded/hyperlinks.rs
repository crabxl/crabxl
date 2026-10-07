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
}
