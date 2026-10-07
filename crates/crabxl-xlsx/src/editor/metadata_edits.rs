//! Metadata edits.
use super::*;

impl<R: Read + Seek> WorkbookEditor<R> {
    pub(crate) fn retained_package_bytes(&self) -> usize {
        self.book
            .retained_source_bytes()
            .saturating_add(size_of::<Self>())
            .saturating_add(self.parts.capacity().saturating_mul(size_of::<PartInfo>()))
            .saturating_add(self.parts.iter().map(|part| part.name.len()).sum::<usize>())
            .saturating_add(self.workbook_relationships.capacity())
            .saturating_add(
                [
                    &self.calc_chain_parts,
                    &self.chain_removals,
                    &self.shared_string_parts,
                ]
                .into_iter()
                .map(|parts| {
                    parts
                        .capacity()
                        .saturating_mul(128)
                        .saturating_add(parts.iter().map(String::capacity).sum::<usize>())
                })
                .sum::<usize>(),
            )
            .saturating_add(self.patch_bytes)
    }
    pub(crate) fn apply_pending_model(
        &mut self,
        name: &str,
        incoming: &mut crabxl_core::Worksheet,
        retained: usize,
        maximum: usize,
    ) -> Result<()> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Source worksheet not found"))?
            .part()
            .to_owned();
        if let Some(patches) = self.patches.get(&part) {
            for patch in patches.values() {
                // Reserve incoming cloned payload and a conservative new cell
                // node before transferring an overlay into the cached model.
                let desired = retained
                    .saturating_add(incoming.charged_bytes())
                    .saturating_add(PATCH_BYTES)
                    .saturating_add(patch.cell.value.heap_bytes());
                self.book.rebalance_strings_for_retained(desired, maximum)?;
                if desired.saturating_add(self.book.retained_source_bytes()) > maximum {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Loaded overlay/model allowance exceeded",
                    ));
                }
                let mut cell = patch.cell.clone();
                cell.style = incoming
                    .get(cell.address)
                    .map_or(StyleId::new(0), |original| original.style);
                incoming.set(cell)?;
            }
        }
        self.book.rebalance_strings_for_retained(
            retained.saturating_add(incoming.charged_bytes()),
            maximum,
        )
    }
    /// Read original canonical view metadata. Pending replacements can be borrowed
    /// separately without cloning their payloads.
    pub fn sheet_views(&mut self, sheet: &str) -> Result<crabxl_core::SheetViews> {
        self.book.sheet_views(sheet)
    }
    /// Borrow a pending display replacement.
    pub fn pending_sheet_views(&self, sheet: &str) -> Option<&crabxl_core::SheetViews> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)?
            .part();
        self.view_patches.get(part).map(Box::as_ref)
    }
    /// Replace viewport metadata while preserving cell values, caches and unrelated
    /// package parts. Unknown source view extensions reject replacement explicitly.
    pub fn set_sheet_views(&mut self, sheet: &str, views: crabxl_core::SheetViews) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed worksheet views requires an explicit signature policy",
            ));
        }
        crate::worksheet_view::validate(&views)?;
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, "Worksheet view source not found")
            })?;
        let part = info.part().to_owned();
        let old = self
            .view_patches
            .get(&part)
            .map_or(0, |views| views.memory_bytes());
        let node = if self.view_patches.contains_key(&part) {
            0
        } else {
            METADATA_ENTRY_BYTES.saturating_add(part.len())
        };
        let bytes = self
            .patch_bytes
            .saturating_sub(old)
            .saturating_add(node)
            .saturating_add(views.memory_bytes());
        if bytes > self.options.max_patch_bytes {
            return Err(limit("Worksheet view overlay allowance exceeded"));
        }
        // Header parsing verifies supported source metadata before it can be replaced.
        // The temporary original model is bounded separately and released immediately.
        let source_allowance = self
            .allowance
            .retained_data_bytes
            .saturating_sub(bytes.max(self.patch_bytes));
        self.book
            .sheet_views_with_allowance(sheet, source_allowance)?;
        self.view_patches.insert(part, Box::new(views));
        self.patch_bytes = bytes;
        Ok(())
    }
    /// Read original printing metadata through worksheet EOF/CRC.
    pub fn print_settings(&mut self, sheet: &str) -> Result<crabxl_core::PrintSettings> {
        self.book.print_settings(sheet)
    }
    /// Borrow a pending canonical printing replacement.
    pub fn pending_print_settings(&self, sheet: &str) -> Option<&crabxl_core::PrintSettings> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)?
            .part();
        self.print_patches.get(part).map(Box::as_ref)
    }
    /// Replace printing metadata while preserving unrelated worksheet/package data.
    /// Printer relationship identity must match the original; graph mutation is separate.
    pub fn set_print_settings(
        &mut self,
        sheet: &str,
        settings: crabxl_core::PrintSettings,
    ) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed printing settings requires an explicit signature policy",
            ));
        }
        crate::printing::validate(&settings)?;
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    "Printing worksheet source not found",
                )
            })?;
        let part = info.part().to_owned();
        let old = self
            .print_patches
            .get(&part)
            .map_or(0, |settings| settings.memory_bytes());
        let node = if self.print_patches.contains_key(&part) {
            0
        } else {
            METADATA_ENTRY_BYTES.saturating_add(part.len())
        };
        let bytes = self
            .patch_bytes
            .saturating_sub(old)
            .saturating_add(node)
            .saturating_add(settings.memory_bytes());
        if bytes > self.options.max_patch_bytes {
            return Err(limit("Printing overlay allowance exceeded"));
        }
        let source_allowance = self
            .allowance
            .retained_data_bytes
            .saturating_sub(bytes.max(self.patch_bytes));
        if let Some(pending) = self.print_patches.get(&part) {
            check_printer_identity(&pending.setup, &settings.setup)?;
        } else {
            let original = self
                .book
                .print_settings_with_allowance(sheet, source_allowance)?;
            check_printer_identity(&original.setup, &settings.setup)?;
        }
        self.print_patches.insert(part, Box::new(settings));
        self.patch_bytes = bytes;
        Ok(())
    }
    /// Update one canonical printing component without cloning unrelated vectors.
    /// The first update validates the original through EOF/CRC; later updates use
    /// the validated overlay without reopening the source. Graph identity remains fixed.
    /// Incoming payloads and parser working buffers are additional to retained overlays.
    pub fn update_print_settings(
        &mut self,
        sheet: &str,
        change: crabxl_core::PrintSettingsChange,
    ) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed printing settings requires an explicit signature policy",
            ));
        }
        crate::printing::validate_change(&change)?;
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    "Printing worksheet source not found",
                )
            })?;
        let part = info.part();
        if let Some(settings) = self.print_patches.get_mut(part) {
            if let crabxl_core::PrintSettingsChange::Setup(setup) = &change {
                check_printer_identity(&settings.setup, setup)?;
            }
            let other = self.patch_bytes.saturating_sub(settings.memory_bytes());
            let maximum = self.options.max_patch_bytes.saturating_sub(other);
            settings.update(change, maximum)?;
            self.patch_bytes = other.saturating_add(settings.memory_bytes());
            return Ok(());
        }
        let part = part.to_owned();
        let other = self
            .patch_bytes
            .saturating_add(METADATA_ENTRY_BYTES)
            .saturating_add(part.len());
        let maximum = self
            .options
            .max_patch_bytes
            .saturating_sub(other)
            .min(self.allowance.retained_data_bytes.saturating_sub(other));
        let mut settings = self.book.print_settings_with_allowance(sheet, maximum)?;
        if let crabxl_core::PrintSettingsChange::Setup(setup) = &change {
            check_printer_identity(&settings.setup, setup)?;
        }
        settings.update(change, maximum)?;
        self.patch_bytes = other.saturating_add(settings.memory_bytes());
        self.print_patches.insert(part, Box::new(settings));
        Ok(())
    }
}
