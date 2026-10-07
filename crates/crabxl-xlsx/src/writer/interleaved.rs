// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

//! Interleaved operations for the single workbook writer owner.
use super::*;

impl WorkbookWriter {
    /// Start an independently appendable sheet, preserving other live spools.
    /// The returned ID is stable until finish/abort. Sheets package in creation
    /// order; buffers, catalogs and temporary bytes remain explicitly bounded.
    pub fn start_interleaved_sheet(&mut self, name: impl Into<String>) -> Result<usize> {
        self.ensure_open()?;
        let previous = self.active.as_ref().map(|sheet| sheet.id);
        if self.active.is_some() {
            self.paused.try_reserve_exact(1).map_err(|error| {
                io_error("Cannot reserve paused worksheet", io::Error::other(error))
            })?;
            if self
                .style_memory_bytes()
                .saturating_add(self.catalog_bytes())
                .saturating_add(self.options.buffer_bytes)
                > self.options.max_metadata_bytes
            {
                return Err(limit("Paused worksheet buffers exceed metadata allowance"));
            }
            self.paused.push(
                self.active
                    .take()
                    .ok_or_else(|| state("No active worksheet"))?,
            );
        }
        if let Err(error) = self.start_sheet(name) {
            if let Some(previous) = previous {
                self.activate_sheet(previous)?;
            }
            return Err(error);
        }
        self.active
            .as_ref()
            .map(|sheet| sheet.id)
            .ok_or_else(|| state("No active worksheet"))
    }
    /// Select a live append-only sheet. Closed or unknown IDs reject explicitly.
    pub fn activate_sheet(&mut self, id: usize) -> Result<()> {
        self.ensure_open()?;
        if self.active.as_ref().is_some_and(|sheet| sheet.id == id) {
            return Ok(());
        }
        let position = self
            .paused
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| state("Worksheet is closed or unknown"))?;
        let selected = self.paused.swap_remove(position);
        if let Some(previous) = self.active.replace(selected) {
            self.paused.push(previous);
        }
        Ok(())
    }
    /// Borrow sparse metadata for a live interleaved sheet.
    pub fn interleaved_dimensions(&self, id: usize) -> Result<&crabxl_core::SheetDimensions> {
        self.ensure_open()?;
        self.active
            .as_ref()
            .filter(|sheet| sheet.id == id)
            .or_else(|| self.paused.iter().find(|sheet| sheet.id == id))
            .map(|sheet| &sheet.dimensions)
            .ok_or_else(|| state("Worksheet is closed or unknown"))
    }
    /// Set row metadata before that row is flushed, without retaining its cells.
    pub fn set_interleaved_row_dimension(
        &mut self,
        id: usize,
        dimension: crabxl_core::RowDimension,
    ) -> Result<()> {
        dimension.validate()?;
        self.validate_dimension_style(dimension.style)?;
        if dimension.descent.is_some() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Extended row descent serialization is not implemented",
            ));
        }
        self.activate_sheet(id)?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.last_row.is_some_and(|row| dimension.index <= row) {
            return Err(state("Cannot change a flushed row dimension"));
        }
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.style_memory_bytes())
            .saturating_sub(
                self.catalog_bytes()
                    .saturating_sub(active.dimensions.heap_bytes()),
            );
        self.active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?
            .dimensions
            .set_row(dimension, allowance)
    }
    /// Set column metadata before the worksheet header is flushed with its first row.
    pub fn set_interleaved_column_dimension(
        &mut self,
        id: usize,
        dimension: crabxl_core::ColumnDimension,
    ) -> Result<()> {
        dimension.validate()?;
        self.validate_dimension_style(dimension.style)?;
        self.activate_sheet(id)?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.columns_written {
            return Err(state("Cannot change flushed column dimensions"));
        }
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.style_memory_bytes())
            .saturating_sub(
                self.catalog_bytes()
                    .saturating_sub(active.dimensions.heap_bytes()),
            );
        self.active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?
            .dimensions
            .set_column(dimension, allowance)
    }
    /// Remove a live unflushed row declaration, retaining reusable metadata capacity.
    pub fn remove_interleaved_row_dimension(&mut self, id: usize, index: RowIndex) -> Result<bool> {
        self.activate_sheet(id)?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.last_row.is_some_and(|last| index <= last) {
            return Err(state("Cannot change a flushed row dimension"));
        }
        Ok(active.dimensions.remove_row(index).is_some())
    }
    /// Remove a column declaration before its header is flushed.
    pub fn remove_interleaved_column_dimension(
        &mut self,
        id: usize,
        index: crabxl_core::ColumnIndex,
    ) -> Result<bool> {
        self.activate_sheet(id)?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.columns_written {
            return Err(state("Cannot change flushed column dimensions"));
        }
        Ok(active.dimensions.remove_column(index).is_some())
    }
    /// Group future rows without retaining any cell values.
    pub fn group_interleaved_rows(
        &mut self,
        id: usize,
        start: RowIndex,
        end: RowIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        self.activate_sheet(id)?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.last_row.is_some_and(|last| start <= last) {
            return Err(state("Cannot change a flushed row dimension"));
        }
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.style_memory_bytes())
            .saturating_sub(
                self.catalog_bytes()
                    .saturating_sub(active.dimensions.heap_bytes()),
            );
        self.active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?
            .dimensions
            .group_rows(start, end, level, hidden, allowance)
    }
    /// Group column declarations before writing the worksheet header.
    pub fn group_interleaved_columns(
        &mut self,
        id: usize,
        start: crabxl_core::ColumnIndex,
        end: crabxl_core::ColumnIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        self.activate_sheet(id)?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.columns_written {
            return Err(state("Cannot change flushed column dimensions"));
        }
        let allowance = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.style_memory_bytes())
            .saturating_sub(
                self.catalog_bytes()
                    .saturating_sub(active.dimensions.heap_bytes()),
            );
        self.active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?
            .dimensions
            .group_columns(start, end, level, hidden, allowance)
    }
    pub(super) fn validate_dimension_style(&self, style: Option<StyleId>) -> Result<()> {
        if style.is_some_and(|style| {
            self.style_catalog()
                .is_none_or(|catalog| catalog.cell_format(style).is_none())
        }) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Unknown dimension style identity",
            ));
        }
        Ok(())
    }
    pub(super) fn flush_column_dimensions(&mut self) -> Result<()> {
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.columns_written {
            return Ok(());
        }
        if active.dimensions.columns().is_empty() {
            self.active
                .as_mut()
                .ok_or_else(|| state("No active worksheet"))?
                .columns_written = true;
            return Ok(());
        }
        let mut header = RowBuffer {
            data: Vec::new(),
            maximum: self
                .options
                .max_metadata_bytes
                .saturating_sub(self.style_memory_bytes())
                .saturating_sub(self.catalog_bytes()),
        };
        crate::dimension_codec::write_columns(&mut header, active.dimensions.columns(), None)
            .map_err(|cause| io_error("Cannot encode column dimensions", cause))?;
        header
            .write_all(b"<sheetData>")
            .map_err(|cause| io_error("Cannot encode worksheet header", cause))?;
        let bytes = active
            .header_prefix_bytes
            .saturating_add(header.data.len() as u64);
        let growth = bytes.saturating_sub(active.bytes);
        let footer = active.footer.as_ref().map_or(FOOTER.len(), Vec::len) as u64;
        if bytes.saturating_add(footer) > self.options.max_sheet_bytes {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        self.check_temp(growth + self.paused_footers() + footer)?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        let rewrite = (|| -> io::Result<()> {
            active
                .output
                .seek(std::io::SeekFrom::Start(active.header_prefix_bytes))?;
            active.output.write_all(&header.data)?;
            active.output.flush()?;
            active.output.get_ref().as_file().set_len(bytes)
        })();
        if let Err(cause) = rewrite {
            self.poisoned = true;
            return Err(io_error("Cannot spool column dimensions", cause));
        }
        active.bytes = bytes;
        active.columns_written = true;
        self.temporary_bytes = self.temporary_bytes.saturating_add(growth);
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        Ok(())
    }
    /// Close one independently appendable sheet without closing other sheets.
    pub fn close_interleaved_sheet(&mut self, id: usize) -> Result<()> {
        self.activate_sheet(id)?;
        self.close_sheet()
    }
    /// Change the metadata name of an existing live or completed sheet.
    pub fn rename_interleaved_sheet(&mut self, id: usize, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        self.ensure_open()?;
        validate_sheet_name(&name)?;
        let folded = name.to_lowercase();
        if self
            .sheets
            .iter()
            .any(|sheet| sheet.id != id && sheet.name.to_lowercase() == folded)
            || self
                .paused
                .iter()
                .any(|sheet| sheet.id != id && sheet.name.to_lowercase() == folded)
            || self
                .active
                .as_ref()
                .is_some_and(|sheet| sheet.id != id && sheet.name.to_lowercase() == folded)
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Worksheet names must be unique ignoring case",
            ));
        }
        if self
            .style_memory_bytes()
            .saturating_add(self.catalog_bytes())
            .saturating_add(name.capacity())
            > self.options.max_metadata_bytes
        {
            return Err(limit("Worksheet name exceeds metadata allowance"));
        }
        let target = self
            .sheets
            .iter_mut()
            .find(|sheet| sheet.id == id)
            .map(|sheet| &mut sheet.name)
            .or_else(|| {
                self.paused
                    .iter_mut()
                    .find(|sheet| sheet.id == id)
                    .map(|sheet| &mut sheet.name)
            })
            .or_else(|| {
                self.active
                    .as_mut()
                    .filter(|sheet| sheet.id == id)
                    .map(|sheet| &mut sheet.name)
            })
            .ok_or_else(|| state("Unknown worksheet ID"))?;
        *target = name;
        Ok(())
    }
}
