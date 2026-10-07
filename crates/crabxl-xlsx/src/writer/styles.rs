// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

//! Styles operations for the single workbook writer owner.
use super::*;

impl WorkbookWriter {
    /// Register a shared basic format, deduplicating identical formats. IDs are
    /// workbook-local; using an ID from another writer is not supported.
    pub fn register_style(&mut self, style: CellStyle) -> Result<StyleId> {
        self.ensure_open()?;
        crate::styles::validate(&style, self.options.max_metadata_bytes)?;
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .register_with_limit(style, allowance)
            .map_err(writer_style_error)
    }
    /// Register a workbook-local named style with shared component identities.
    pub fn register_named_style(
        &mut self,
        name: Box<str>,
        style: StyleId,
        options: crabxl_core::NamedStyleOptions,
    ) -> Result<StyleId> {
        self.ensure_open()?;
        validate_xml_text(&name)?;
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .register_named_style_with_limit(name, style, options, allowance)
            .map_err(writer_style_error)
    }
    /// Rename/update a named declaration without rewriting previously spooled rows.
    pub fn update_named_metadata(
        &mut self,
        name: &str,
        new_name: Box<str>,
        options: crabxl_core::NamedStyleOptions,
    ) -> Result<()> {
        self.ensure_open()?;
        validate_xml_text(&new_name)?;
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .update_named_metadata_with_limit(name, new_name, options, allowance)
            .map_err(writer_style_error)
    }
    /// Update a named base appearance while retaining previously written formats.
    pub fn update_named_style(&mut self, name: &str, style: StyleId) -> Result<StyleId> {
        self.ensure_open()?;
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .update_named_style_with_limit(name, style, allowance)
            .map_err(writer_style_error)
    }
    /// Resolve and deduplicate a registered named style's cell format.
    pub fn named_style_format(&mut self, name: &str) -> Result<StyleId> {
        self.ensure_open()?;
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .named_style_format_with_limit(name, allowance)
            .map_err(writer_style_error)
    }
    /// Replace one component of a workbook-local format without copying its peers.
    pub fn derive_style_component(
        &mut self,
        base: StyleId,
        component: crabxl_core::StyleComponent,
    ) -> Result<StyleId> {
        self.ensure_open()?;
        if let crabxl_core::StyleComponent::Font(font) = &component
            && let Some(name) = &font.name
        {
            validate_xml_text(name)?;
        }
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .derive_component_with_limit(base, component, allowance)
            .map_err(writer_style_error)
    }
    /// Register a literal code against imported declarations and built-in overrides.
    pub fn register_number_format(&mut self, code: Box<str>) -> Result<u32> {
        self.ensure_open()?;
        validate_xml_text(&code)?;
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .register_number_format_with_limit(code, allowance)
            .map_err(writer_style_error)
    }
    /// Register a source format using existing components, retaining its explicit flags.
    pub fn register_format(&mut self, format: crabxl_core::CellFormat) -> Result<StyleId> {
        self.ensure_open()?;
        if format.unmodeled_extensions {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Unmodeled style extensions require original-package preservation",
            ));
        }
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        self.styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?
            .register_format_with_limit(format, allowance)
            .map_err(writer_style_error)
    }
    /// Borrow the canonical shared catalog; None after abort releases storage.
    pub fn style_catalog(&self) -> Option<&StyleCatalog> {
        self.styles.as_ref().map(StyleRegistry::catalog)
    }
    /// Managed catalog and conservative registration-index storage estimate.
    pub fn style_memory_bytes(&self) -> usize {
        self.styles.as_ref().map_or(0, StyleRegistry::memory_bytes)
    }
    /// Replace the output theme policy before packaging under the metadata allowance.
    pub fn set_theme(&mut self, theme: crate::ThemeWritePolicy) -> Result<()> {
        self.ensure_open()?;
        let retained = self
            .catalog_bytes()
            .saturating_sub(self.options.theme.memory_bytes())
            .saturating_add(self.style_memory_bytes())
            .saturating_add(theme.memory_bytes());
        if retained > self.options.max_metadata_bytes {
            return Err(limit("Theme exceeds writer metadata allowance"));
        }
        if let crate::ThemeWritePolicy::Validated(theme) = &theme {
            crate::theme::validate(
                theme.bytes(),
                "xl/theme/theme1.xml",
                crabxl_core::ResourceLimits {
                    max_theme_bytes: self.options.max_metadata_bytes,
                    ..Default::default()
                },
            )?;
        }
        self.options.theme = theme;
        Ok(())
    }
    /// Borrow explicitly supplied theme bytes; standard/omitted themes have no owned payload.
    pub fn theme(&self) -> Option<&crabxl_core::Theme> {
        match &self.options.theme {
            crate::ThemeWritePolicy::Custom(theme) | crate::ThemeWritePolicy::Validated(theme) => {
                Some(theme)
            }
            _ => None,
        }
    }
    /// Managed custom-theme storage; the default theme uses static storage.
    pub fn theme_memory_bytes(&self) -> usize {
        self.options.theme.memory_bytes()
    }
    pub(super) fn catalog_bytes(&self) -> usize {
        self.options.theme.memory_bytes()
            + self.paused_bytes()
            + self.sheets.capacity() * size_of::<StoredSheet>()
            + self
                .sheets
                .iter()
                .map(|sheet| {
                    sheet.name.capacity()
                        + sheet.file.path().as_os_str().len()
                        + sheet.relationships.as_ref().map_or(0, Vec::capacity)
                        + sheet
                            .relationship_spool
                            .as_ref()
                            .map_or(0, |file| file.path().as_os_str().len())
                })
                .sum::<usize>()
            + self.active.as_ref().map_or(0, |sheet| {
                sheet.name.capacity()
                    + sheet.output.get_ref().path().as_os_str().len()
                    + sheet.footer.as_ref().map_or(0, Vec::capacity)
                    + sheet.relationships.as_ref().map_or(0, Vec::capacity)
                    + sheet.dimensions.heap_bytes()
                    + sheet
                        .link_spool
                        .as_ref()
                        .map_or(0, hyperlinks::LinkSpool::heap_bytes)
            })
    }
    pub(super) fn style_bytes(&self) -> usize {
        self.style_memory_bytes()
            .saturating_add(self.options.theme.memory_bytes())
    }
    pub(super) fn paused_bytes(&self) -> usize {
        self.paused.capacity() * size_of::<ActiveSheet>()
            + self
                .paused
                .iter()
                .map(|sheet| {
                    sheet.name.capacity()
                        + sheet.output.capacity()
                        + sheet.output.get_ref().path().as_os_str().len()
                        + sheet.dimensions.heap_bytes()
                        + sheet.footer.as_ref().map_or(0, Vec::capacity)
                        + sheet.relationships.as_ref().map_or(0, Vec::capacity)
                        + sheet
                            .link_spool
                            .as_ref()
                            .map_or(0, hyperlinks::LinkSpool::heap_bytes)
                })
                .sum::<usize>()
    }
    pub(super) fn paused_footers(&self) -> u64 {
        self.paused
            .iter()
            .map(|sheet| {
                sheet.footer.as_ref().map_or(FOOTER.len(), Vec::len) as u64
                    + sheet
                        .link_spool
                        .as_ref()
                        .map_or(0, hyperlinks::LinkSpool::pending_bytes)
            })
            .sum()
    }
}
