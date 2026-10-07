// SPDX-License-Identifier: MIT
// Workbook/relationship parsing adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Catalogs operations.
use super::*;

impl<R: Read + Seek> WorkbookReader<R> {
    /// Lazily load and borrow exact theme bytes, without materializing drawing graphs.
    /// An absent relationship returns None. Unknown valid theme sections are retained.
    pub fn theme(&mut self) -> Result<Option<&crabxl_core::Theme>> {
        if self.theme.is_none()
            && let Some(part) = &self.theme_part
        {
            let mut file = self.archive.by_name(part).map_err(|error| {
                Error::caused_by(ErrorKind::Archive, "Cannot open theme", error).with_part(part)
            })?;
            let size = file.size();
            let maximum = self
                .limits
                .max_part_bytes
                .min(self.style_metadata_remaining)
                .min(self.limits.max_theme_bytes as u64);
            if size > maximum || size > isize::MAX as u64 {
                return Err(
                    limit("Combined theme metadata input exceeds allowance").with_part(part)
                );
            }
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(size as usize).map_err(|error| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot reserve theme bytes",
                    error,
                )
                .with_part(part)
            })?;
            bytes.resize(size as usize, 0);
            file.read_exact(&mut bytes).map_err(|error| {
                Error::caused_by(ErrorKind::Io, "Cannot read theme", error).with_part(part)
            })?;
            let mut extra = [0];
            if file.read(&mut extra).map_err(|error| {
                Error::caused_by(ErrorKind::Io, "Cannot finish theme and verify CRC", error)
                    .with_part(part)
            })? != 0
            {
                return Err(invalid("Theme size differs from ZIP declaration").with_part(part));
            }
            self.theme = Some(crabxl_core::Theme::from_bytes(bytes.into_boxed_slice()));
            self.style_metadata_remaining -= size;
        }
        Ok(self.theme.as_ref())
    }
    /// Decode a caller-owned palette/font catalog on explicit request. Opaque
    /// theme access remains unchanged. The returned catalog is not cached and
    /// caller retention is outside subsequent reader accounting. Parsing obeys
    /// the remaining aggregate metadata and theme payload allowances.
    pub fn read_theme_catalog(&mut self) -> Result<Option<crabxl_core::ThemeCatalog>> {
        self.theme()?;
        match (&self.theme, &self.theme_part) {
            (Some(theme), Some(part)) => {
                let maximum = self
                    .limits
                    .max_theme_bytes
                    .saturating_sub(theme.memory_bytes())
                    .min(usize::try_from(self.style_metadata_remaining).unwrap_or(usize::MAX));
                crate::theme::catalog(theme.bytes(), part, self.limits, maximum).map(Some)
            }
            _ => Ok(None),
        }
    }
    /// Explicitly validate a prepared theme as bounded DrawingML XML.
    /// Ordinary theme access retains opaque bytes, matching the public baseline.
    pub fn validate_theme(&mut self) -> Result<()> {
        self.theme()?;
        if let (Some(theme), Some(part)) = (&self.theme, &self.theme_part) {
            crate::theme::validate(theme.bytes(), part, self.limits)?;
        }
        Ok(())
    }
    /// Managed retained theme bytes; zero before lazy preparation or when absent.
    pub fn theme_memory_bytes(&self) -> usize {
        self.theme
            .as_ref()
            .map_or(0, crabxl_core::Theme::memory_bytes)
    }
    /// Managed package names/catalogs and prepared style/theme payloads.
    /// ZIP dependency allocations and shared-string storage are separate.
    pub fn catalog_memory_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(
                self.sheets
                    .capacity()
                    .saturating_mul(size_of::<SheetInfo>()),
            )
            .saturating_add(
                self.sheets
                    .iter()
                    .map(|s| s.name.capacity().saturating_add(s.part.capacity()))
                    .sum::<usize>(),
            )
            .saturating_add(self.workbook_part.capacity())
            .saturating_add(
                [&self.shared_string_part, &self.style_part, &self.theme_part]
                    .into_iter()
                    .flatten()
                    .map(String::capacity)
                    .sum::<usize>(),
            )
            .saturating_add(
                self.shared_string_options
                    .temp_directory
                    .as_ref()
                    .map_or(0, |path| path.capacity()),
            )
            .saturating_add(self.style_memory_bytes())
            .saturating_add(self.theme_memory_bytes())
    }
    pub(crate) fn policy_catalog_bytes(&self) -> usize {
        self.catalog_memory_bytes().saturating_add(
            self.shared_strings
                .as_ref()
                .map_or(0, SharedStrings::minimum_managed_bytes),
        )
    }
    pub(crate) fn rebalance_strings_for_retained(
        &mut self,
        desired: usize,
        allowance: usize,
    ) -> Result<()> {
        if self.shared_string_options.storage == crate::SharedStringStorage::Memory {
            return Ok(());
        }
        let Some(maximum) = allowance
            .checked_sub(self.catalog_memory_bytes())
            .and_then(|n| n.checked_sub(desired))
        else {
            return Ok(());
        };
        if let Some(strings) = &mut self.shared_strings {
            strings.limit_or_spill(&self.shared_string_options, maximum)?;
        }
        Ok(())
    }
    pub(crate) fn shared_cache_bytes(&self) -> usize {
        self.shared_strings.as_ref().map_or(0, |strings| {
            strings
                .stats()
                .managed_bytes
                .saturating_sub(strings.minimum_managed_bytes())
        })
    }
    pub(crate) fn retained_source_bytes(&self) -> usize {
        self.catalog_memory_bytes().saturating_add(
            self.shared_strings
                .as_ref()
                .map_or(0, |strings| strings.stats().managed_bytes),
        )
    }
    /// Load and borrow the shared style catalog without materializing a worksheet.
    /// Unknown/staged root sections remain explicitly listed; original-package
    /// preservation does not imply typed support for those sections.
    pub fn style_catalog(&mut self) -> Result<Option<&crabxl_core::StyleCatalog>> {
        if self.styles_transferred {
            return Err(invalid("Source styles belong to the loaded workbook bank"));
        }
        self.prepare_styles()?;
        Ok(self.imported_styles.as_ref().map(|s| &s.catalog))
    }
    /// Consume this reader and transfer its validated style catalog without cloning.
    /// Remaining archive/string-cache resources close when the reader is consumed.
    /// Derived date lookups are discarded; canonical format records retain their kinds.
    pub fn into_style_catalog(mut self) -> Result<Option<crabxl_core::StyleCatalog>> {
        if self.styles_transferred {
            return Err(invalid("Source styles belong to the loaded workbook bank"));
        }
        self.prepare_styles()?;
        Ok(self.imported_styles.take().map(|styles| styles.catalog))
    }
    pub(crate) fn transfer_style_catalog(
        &mut self,
        maximum: usize,
    ) -> Result<Option<crabxl_core::StyleCatalog>> {
        if self.styles_transferred {
            return Err(invalid("Source styles already transferred"));
        }
        self.prepare_styles_with_allowance(Some(maximum))?;
        self.styles_transferred = true;
        Ok(self
            .imported_styles
            .as_mut()
            .map(|styles| std::mem::take(&mut styles.catalog)))
    }
    /// Retained style catalog plus derived number-format classifications.
    pub fn style_memory_bytes(&self) -> usize {
        self.imported_styles
            .as_ref()
            .map_or(0, |s| s.memory_bytes())
    }
    pub(super) fn prepare_styles(&mut self) -> Result<()> {
        self.prepare_styles_with_allowance(None)
    }
    pub(super) fn prepare_styles_with_allowance(&mut self, allowance: Option<usize>) -> Result<()> {
        if self.imported_styles.is_some() {
            if allowance.is_some_and(|maximum| self.style_memory_bytes() > maximum) {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Prepared styles exceed aggregate allowance",
                ));
            }
            return Ok(());
        }
        let Some(part) = &self.style_part else {
            return Ok(());
        };
        let file = self.archive.by_name(part).map_err(|e| {
            Error::caused_by(ErrorKind::Archive, "Cannot open style catalog", e).with_part(part)
        })?;
        let style_input_bytes = file.size();
        let maximum = self
            .limits
            .max_part_bytes
            .min(self.style_metadata_remaining);
        if file.size() > maximum {
            return Err(limit("Combined style metadata input exceeds allowance").with_part(part));
        }
        let mut style_limits = self.limits;
        style_limits.max_part_bytes = maximum;
        let style_allowance = allowance
            .unwrap_or(self.limits.max_style_bytes)
            .min(self.limits.max_style_bytes);
        let catalog = crate::style_reader::read(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part.clone(),
            style_limits,
            style_allowance,
            self.limits.max_style_records,
        )?;
        let imported = crate::style_reader::ImportedStyles::new(catalog, style_allowance)
            .map_err(|e| e.with_part(part))?;
        self.imported_styles = Some(imported);
        self.style_metadata_remaining -= style_input_bytes;
        Ok(())
    }
}
