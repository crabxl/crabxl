// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

//! Rows operations for the single workbook writer owner.
use super::*;

impl WorkbookWriter {
    /// Write a complete sparse row. Validate/encode before spooling so invalid
    /// rows and budget failures do not partly commit worksheet content.
    pub fn write_row(&mut self, row: &Row) -> Result<()> {
        loop {
            let active = self
                .active
                .as_ref()
                .ok_or_else(|| state("No active worksheet"))?;
            let dimensions = active.dimensions.rows();
            let next = active.last_row.map_or(0, |last| {
                dimensions.partition_point(|dimension| dimension.index <= last)
            });
            let dimension = dimensions
                .get(next)
                .filter(|dimension| dimension.index < row.index)
                .cloned();
            let Some(dimension) = dimension else {
                break;
            };
            self.write_cells_with_dimension(dimension.index, std::iter::empty(), Some(&dimension))?;
        }
        let dimension = self
            .active
            .as_ref()
            .and_then(|sheet| sheet.dimensions.row(row.index))
            .cloned();
        self.write_cells_with_dimension(row.index, row.cells.iter(), dimension.as_ref())
    }
    /// Write an explicitly materialized sparse sheet without cloning cell payloads.
    /// Style IDs must refer to this writer's registered formats. This creates a
    /// new sheet; it does not preserve parts of a loaded source package.
    pub fn write_worksheet(&mut self, sheet: &crabxl_core::Worksheet) -> Result<()> {
        sheet.dimensions().validate()?;
        if sheet
            .dimensions()
            .rows()
            .iter()
            .any(|row| row.descent.is_some())
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Extended row descent serialization is not implemented",
            ));
        }
        self.start_sheet_with_dimensions(
            sheet.name(),
            sheet.sheet_views(),
            sheet.print_settings(),
            sheet.dimensions().columns(),
        )?;
        let id = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?
            .id;
        self.set_sheet_visibility(id, sheet.visibility())?;
        let mut last = None;
        let mut cells = sheet.row_indices().peekable();
        let mut dimensions = sheet
            .dimensions()
            .rows()
            .iter()
            .map(|row| row.index)
            .peekable();
        while cells.peek().is_some() || dimensions.peek().is_some() {
            let index = match (cells.peek(), dimensions.peek()) {
                (Some(cell), Some(dimension)) => (*cell).min(*dimension),
                (Some(cell), None) => *cell,
                (None, Some(dimension)) => *dimension,
                (None, None) => break,
            };
            if cells.peek() == Some(&index) {
                cells.next();
            }
            if dimensions.peek() == Some(&index) {
                dimensions.next();
            }
            self.write_cells_with_dimension(
                index,
                sheet.row_cells(index),
                sheet.dimensions().row(index),
            )?;
            last = Some(index.get());
        }
        if sheet.row_extent() > 0 && last.is_none_or(|last| last + 1 < sheet.row_extent()) {
            self.write_row(&Row::new(RowIndex::new(sheet.row_extent() - 1)?))?;
        }
        self.close_sheet()
    }
    /// Spool an owned workbook's borrowed sheets in display order. The writer
    /// must be fresh; caller-registered style IDs are shared with the models.
    /// An owned catalog must equal the writer's catalog before any output starts;
    /// matching catalogs support repeated borrowed saves without payload cloning.
    /// Model epoch and active sheet are applied before any worksheet starts.
    pub fn write_workbook(&mut self, workbook: &crabxl_core::Workbook) -> Result<()> {
        self.ensure_open()?;
        if !self.sheets.is_empty() || !self.paused.is_empty() || self.active.is_some() {
            return Err(state("Workbook model export requires a fresh writer"));
        }
        if let Some(catalog) = workbook.style_catalog()
            && self
                .styles
                .as_ref()
                .is_none_or(|styles| styles.catalog() != catalog)
        {
            return Err(state(
                "Borrowed styled export requires an identical writer catalog or from_workbook ownership transfer",
            ));
        }
        if workbook.is_empty() {
            return Err(Error::new(
                ErrorKind::NoVisibleSheet,
                "A workbook requires at least one worksheet",
            ));
        }
        if let Some(theme) = workbook.theme() {
            let bytes = theme
                .memory_bytes()
                .saturating_add(self.style_memory_bytes());
            if bytes > self.options.max_metadata_bytes {
                return Err(limit("Workbook theme exceeds writer metadata allowance"));
            }
            if matches!(self.options.theme, crate::ThemeWritePolicy::Validated(_)) {
                crate::theme::validate(
                    theme.bytes(),
                    "xl/theme/theme1.xml",
                    crabxl_core::ResourceLimits {
                        max_theme_bytes: self.options.max_metadata_bytes,
                        max_part_bytes: self.options.max_metadata_bytes as u64,
                        ..Default::default()
                    },
                )?;
            }
            self.options.theme =
                if matches!(self.options.theme, crate::ThemeWritePolicy::Validated(_)) {
                    crate::ThemeWritePolicy::Validated(theme.clone())
                } else {
                    crate::ThemeWritePolicy::Custom(theme.clone())
                };
        }
        self.options.date_1904 = workbook.epoch() == DateEpoch::Mac1904;
        self.options.active_sheet = workbook.active_index().unwrap_or(0);
        let original = self.canonical_styles;
        self.canonical_styles = workbook.style_catalog().is_some();
        let result = (|| {
            for (_, sheet) in workbook.sheets() {
                self.write_worksheet(sheet)?;
            }
            Ok(())
        })();
        self.canonical_styles = original;
        result
    }
    pub(super) fn write_cells_with_dimension<'a>(
        &mut self,
        index: RowIndex,
        cells: impl Iterator<Item = &'a crabxl_core::Cell> + Clone,
        dimension: Option<&crabxl_core::RowDimension>,
    ) -> Result<()> {
        self.ensure_open()?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("Start a worksheet before writing rows"))?;
        if active.last_row.is_some_and(|previous| index <= previous) {
            return Err(state("Sequential writer cannot revisit a flushed row"));
        }
        let part = format!("xl/worksheets/sheet{}.xml", self.sheets.len() + 1);
        let style_allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes());
        let styles = self
            .styles
            .as_mut()
            .ok_or_else(|| state("Writer style catalog is released"))?;
        let context = if self.canonical_styles {
            StyleContext::Canonical(styles.catalog())
        } else {
            StyleContext::Registry {
                registry: styles,
                maximum: style_allowance,
            }
        };
        crate::encode::encode_cells_with_dimension(
            &mut self.row_buffer,
            (index, dimension),
            cells.clone(),
            self.options.max_cell_bytes,
            self.options.max_row_cells,
            context,
            ValueEncoding {
                epoch: if self.options.date_1904 {
                    DateEpoch::Mac1904
                } else {
                    DateEpoch::Windows1900
                },
                iso_dates: self.options.iso_dates,
                non_finite: self.options.non_finite,
                formula_attributes: self.options.formula_attributes,
                date_styles: self.date_styles,
                invalidate_caches: false,
            },
        )
        .map_err(|error| error.with_part(&part))?;
        self.flush_column_dimensions()?;
        let length = self.row_buffer.data.len() as u64;
        self.check_temp(
            length
                + self.paused_footers()
                + self.active.as_ref().map_or(FOOTER.len(), |sheet| {
                    sheet.footer.as_ref().map_or(FOOTER.len(), Vec::len)
                }) as u64,
        )
        .map_err(|error| error.with_part(&part))?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        if active
            .bytes
            .saturating_add(length)
            .saturating_add(active.footer.as_ref().map_or(FOOTER.len(), Vec::len) as u64)
            > self.options.max_sheet_bytes
        {
            return Err(limit("Writer sheet byte limit exceeded").with_part(part));
        }
        if let Err(error) = active.output.write_all(&self.row_buffer.data) {
            self.poisoned = true;
            return Err(io_error("Cannot spool worksheet row", error).with_part(part));
        }
        active.bytes += length;
        active.last_row = Some(index);
        self.temporary_bytes += length;
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        self.stats.rows += 1;
        self.stats.cells += cells.count() as u64;
        Ok(())
    }
    /// Finish the active sheet and flush its temporary XML; does not publish ZIP.
    /// Calling this without an active sheet is harmless.
    pub fn close_sheet(&mut self) -> Result<()> {
        self.ensure_open()?;
        if self.active.is_none() {
            return Ok(());
        }
        self.flush_column_dimensions()?;
        loop {
            let active = self
                .active
                .as_ref()
                .ok_or_else(|| state("No active worksheet"))?;
            let rows = active.dimensions.rows();
            let next = active
                .last_row
                .map_or(0, |last| rows.partition_point(|row| row.index <= last));
            let dimension = rows.get(next).cloned();
            let Some(dimension) = dimension else {
                break;
            };
            self.write_cells_with_dimension(dimension.index, std::iter::empty(), Some(&dimension))?;
        }
        let footer = self.active.as_mut().and_then(|sheet| sheet.footer.take());
        self.write_active(footer.as_deref().unwrap_or(FOOTER))?;
        let active = self
            .active
            .take()
            .ok_or_else(|| state("No active worksheet"))?;
        let file = active.output.into_inner().map_err(|error| {
            self.poisoned = true;
            io_error("Cannot flush worksheet temporary file", error.into_error())
        })?;
        self.sheets.push(StoredSheet {
            id: active.id,
            name: active.name,
            file,
            visibility: active.visibility,
        });
        Ok(())
    }
}
