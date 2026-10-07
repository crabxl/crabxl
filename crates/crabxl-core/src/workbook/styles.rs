//! Styles operations.
use super::*;

impl Workbook {
    /// Borrow canonical workbook-local style identities, if explicitly initialized.
    pub fn style_catalog(&self) -> Option<&crate::StyleCatalog> {
        self.styles.as_ref().map(crate::StyleRegistry::catalog)
    }
    /// Adopt source style identities before registering styles. Existing cells must
    /// refer to the imported table. Failure preserves this workbook's model state.
    pub fn import_style_catalog(
        &mut self,
        catalog: crate::StyleCatalog,
        mut limits: crate::StyleLimits,
    ) -> Result<()> {
        if self.styles.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidState,
                "Workbook styles already initialized",
            ));
        }
        for (_, sheet) in self.sheets() {
            sheet.validate_style_links(Some(&catalog))?;
        }
        let requested = limits;
        limits.max_bytes = limits
            .max_bytes
            .min(self.limits.max_bytes.saturating_sub(self.charged_bytes()));
        let mut styles = crate::StyleRegistry::from_catalog(catalog, limits)?;
        styles.set_limits(requested)?;
        self.styles = Some(styles);
        Ok(())
    }
    /// Register appearance using the same aggregate allowance as all worksheets.
    /// An initial default registry is created lazily; existing source IDs are retained.
    pub fn register_style(&mut self, style: crate::CellStyle) -> Result<crate::StyleId> {
        let maximum = self.style_allowance();
        if let Some(styles) = &mut self.styles {
            return styles.register_with_limit(style, maximum);
        }
        let mut styles = crate::StyleRegistry::new(crate::StyleLimits {
            max_bytes: maximum,
            ..Default::default()
        })?;
        let id = styles.register(style)?;
        styles.set_limits(crate::StyleLimits {
            max_bytes: self.limits.max_bytes,
            ..Default::default()
        })?;
        self.styles = Some(styles);
        Ok(id)
    }
    /// Register a raw format referencing this bank's imported/shared components.
    pub fn register_format(&mut self, format: crate::CellFormat) -> Result<crate::StyleId> {
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidState,
                    "Workbook styles are not initialized",
                )
            })?
            .register_format_with_limit(format, maximum)
    }
    /// Intern a literal number-format code without rebuilding component payloads.
    pub fn register_number_format(&mut self, code: Box<str>) -> Result<u32> {
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidState,
                    "Workbook styles are not initialized",
                )
            })?
            .register_number_format_with_limit(code, maximum)
    }
    /// Register a unique named appearance under the joint workbook allowance.
    pub fn register_named_style(
        &mut self,
        name: Box<str>,
        style: crate::StyleId,
        options: crate::NamedStyleOptions,
    ) -> Result<crate::StyleId> {
        if self.styles.is_none() {
            if style.get() != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unknown named appearance identity",
                ));
            }
            self.register_style(crate::CellStyle::default())?;
        }
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Missing canonical styles"))?
            .register_named_style_with_limit(name, style, options, maximum)
    }
    /// Resolve a named appearance into a workbook-local cell format.
    pub fn named_style_format(&mut self, name: &str) -> Result<crate::StyleId> {
        if self.styles.is_none() {
            self.register_style(crate::CellStyle::default())?;
        }
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Missing canonical styles"))?
            .named_style_format_with_limit(name, maximum)
    }
    /// Rename/update a named declaration under the shared workbook allowance.
    pub fn update_named_metadata(
        &mut self,
        name: &str,
        new_name: Box<str>,
        options: crate::NamedStyleOptions,
    ) -> Result<()> {
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown named style"))?
            .update_named_metadata_with_limit(name, new_name, options, maximum)
    }
    /// Update a registered named appearance without modifying existing cell formats.
    pub fn update_named_style(
        &mut self,
        name: &str,
        style: crate::StyleId,
    ) -> Result<crate::StyleId> {
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown named style"))?
            .update_named_style_with_limit(name, style, maximum)
    }
    /// Merge a finite rectangle using shared virtual border/protection styles.
    pub fn merge_cells(&mut self, id: SheetId, range: crate::CellRange) -> Result<()> {
        crate::CellRange::new(range.start, range.end)?;
        let sheet = self.sheet(id)?;
        if sheet.merged_ranges().contains(range) {
            return Ok(());
        }
        let anchor = sheet.style_at(range.start);
        let corner = (sheet.get(range.end).is_some()
            || sheet.merged_ranges().virtual_style(range.end).is_some())
        .then(|| sheet.style_at(range.end));
        let (prepared, anchor_style) = self.prepare_merge(range, anchor, corner)?;
        self.sheet_mut(id)?.merge_prepared(prepared, anchor_style)
    }
    /// Resolve shared appearances without mutating a caller's materialized sheet.
    /// Coordinators must reserve that sheet's bytes in the workbook allowance
    /// before preparing styles, then apply the result under its own sheet limit.
    pub fn prepare_merge(
        &mut self,
        range: crate::CellRange,
        anchor: crate::StyleId,
        corner: Option<crate::StyleId>,
    ) -> Result<(crate::MergedCellRange, crate::StyleId)> {
        crate::CellRange::new(range.start, range.end)?;
        if self.styles.is_none() {
            self.register_style(crate::CellStyle::default())?;
        }
        let catalog = self
            .style_catalog()
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Missing merge style owner"))?;
        let appearance = catalog.cell_style(anchor)?;
        let original = appearance.border.clone();
        let mut border = original.clone();
        let protection = appearance.protection.copied();
        let default = catalog.cell_style(crate::StyleId::new(0))?;
        let default_border = default.border.clone();
        let default_protection = default.protection.copied();
        if let Some(corner) = corner {
            let corner = catalog.cell_style(corner)?;
            for side in [1, 3] {
                merge_border_side(&mut border.sides[side], &corner.border.sides[side]);
            }
        }
        let anchor_style = if border == original {
            anchor
        } else {
            self.derive_style_component(
                anchor,
                crate::StyleComponent::Border(Box::new(border.clone())),
            )?
        };
        let mut appearances = [crate::StyleId::new(0); 16];
        for (mask, style) in appearances.iter_mut().enumerate() {
            let available = |bits, size| match size {
                1 => bits == 3,
                2 => bits == 1 || bits == 2,
                _ => bits != 3,
            };
            if !available(
                mask & 3,
                range.end.column.get() - range.start.column.get() + 1,
            ) || !available(mask >> 2, range.end.row.get() - range.start.row.get() + 1)
            {
                continue;
            }
            let mut edge = default_border.clone();
            for side in 0..4 {
                if mask & (1 << side) != 0
                    && border.sides[side].as_ref().is_some_and(|side| {
                        side.line
                            .is_some_and(|line| line != crate::BorderLine::None)
                    })
                {
                    merge_border_side(&mut edge.sides[side], &border.sides[side]);
                }
            }
            if edge != default_border {
                *style = self.derive_style_component(
                    *style,
                    crate::StyleComponent::Border(Box::new(edge)),
                )?;
            }
            let effective_protection = |value: Option<crate::Protection>| {
                let value = value.unwrap_or_default();
                (value.locked.unwrap_or(true), value.hidden.unwrap_or(false))
            };
            if effective_protection(protection) != effective_protection(default_protection) {
                *style = self.derive_style_component(
                    *style,
                    crate::StyleComponent::Protection(protection),
                )?;
            }
        }
        let prepared = crate::MergedCellRange::new(range, appearances)?;
        Ok((prepared, anchor_style))
    }
    /// Derive a format by replacing one appearance component under the bank cap.
    /// All other component identities, base links and flags remain unchanged.
    pub fn derive_style_component(
        &mut self,
        style: crate::StyleId,
        component: crate::StyleComponent,
    ) -> Result<crate::StyleId> {
        if self.styles.is_none() {
            if style.get() != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unknown workbook style identity",
                ));
            }
            self.register_style(crate::CellStyle::default())?;
        }
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Missing canonical styles"))?
            .derive_component_with_limit(style, component, maximum)
    }
    /// Derive a cell format by changing only its number-format code.
    /// Component identities, inheritance and unrelated flags remain unchanged.
    pub fn derive_number_format(
        &mut self,
        style: crate::StyleId,
        code: Box<str>,
    ) -> Result<crate::StyleId> {
        if self.styles.is_none() {
            if style.get() != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unknown workbook style identity",
                ));
            }
            self.register_style(crate::CellStyle::default())?;
        }
        let mut format = self
            .style_catalog()
            .and_then(|catalog| catalog.cell_format(style))
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown workbook style identity"))?
            .clone();
        if self
            .style_catalog()
            .and_then(|catalog| catalog.number_format(format.number_format_id))
            == Some(code.as_ref())
        {
            return Ok(style);
        }
        format.number_format_id = self.register_number_format(code)?;
        format.apply_number_format = Some(true);
        self.register_format(format)
    }
    pub(super) fn style_allowance(&self) -> usize {
        self.limits
            .max_bytes
            .saturating_sub(self.charged_bytes().saturating_sub(self.style_bytes()))
    }
    pub(super) fn style_bytes(&self) -> usize {
        self.styles
            .as_ref()
            .map_or(0, crate::StyleRegistry::memory_bytes)
    }
}
