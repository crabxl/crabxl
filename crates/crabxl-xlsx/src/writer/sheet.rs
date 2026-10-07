// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

//! Sheet operations for the single workbook writer owner.
use super::*;

impl WorkbookWriter {
    /// Start a sheet, completing the previous one. Failed validation or temporary
    /// file creation leaves the previous active sheet usable.
    pub fn start_sheet(&mut self, name: impl Into<String>) -> Result<()> {
        self.start_sheet_with_header(name.into(), HEADER, None, 0)
    }
    /// Set catalog visibility for an open, paused or completed worksheet spool.
    /// The stable writer identity survives rename and scheduling changes. At
    /// least one completed sheet must be visible when packaging output.
    pub fn set_sheet_visibility(
        &mut self,
        id: usize,
        visibility: crabxl_core::SheetVisibility,
    ) -> Result<()> {
        self.ensure_open()?;
        let target = self
            .sheets
            .iter_mut()
            .find(|sheet| sheet.id == id)
            .map(|sheet| &mut sheet.visibility)
            .or_else(|| {
                self.paused
                    .iter_mut()
                    .find(|sheet| sheet.id == id)
                    .map(|sheet| &mut sheet.visibility)
            })
            .or_else(|| {
                self.active
                    .as_mut()
                    .filter(|sheet| sheet.id == id)
                    .map(|sheet| &mut sheet.visibility)
            })
            .ok_or_else(|| state("Unknown worksheet ID"))?;
        *target = visibility;
        Ok(())
    }
    /// Start a sheet with borrowed canonical viewport metadata. Validation and
    /// bounded encoding precede any active-sheet closure or spool creation.
    pub fn start_sheet_with_views(
        &mut self,
        name: impl Into<String>,
        views: &crabxl_core::SheetViews,
    ) -> Result<()> {
        self.start_sheet_with_settings(name, Some(views), None)
    }
    /// Borrow canonical views and printing settings. Encoded metadata is bounded;
    /// printing relationships require the later new-package relationship graph.
    pub fn start_sheet_with_settings(
        &mut self,
        name: impl Into<String>,
        views: Option<&crabxl_core::SheetViews>,
        printing: Option<&crabxl_core::PrintSettings>,
    ) -> Result<()> {
        self.start_sheet_with_dimensions(name, views, printing, &[], &[], &Default::default())
    }
    pub(super) fn start_sheet_with_dimensions(
        &mut self,
        name: impl Into<String>,
        views: Option<&crabxl_core::SheetViews>,
        printing: Option<&crabxl_core::PrintSettings>,
        columns: &[crabxl_core::ColumnDimension],
        merges: &[crabxl_core::MergedCellRange],
        hyperlinks: &crabxl_core::Hyperlinks,
    ) -> Result<()> {
        self.ensure_open()?;
        if views.is_none()
            && printing.is_none()
            && columns.is_empty()
            && merges.is_empty()
            && hyperlinks.is_empty()
        {
            return self.start_sheet(name);
        }
        if let Some(views) = views {
            crate::worksheet_view::validate(views)?;
        }
        if let Some(printing) = printing {
            crate::printing::validate(printing)?;
            if printing.setup.printer_relationship.is_some() {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "New printer relationships require a package feature graph",
                ));
            }
        }
        for column in columns {
            column.validate()?;
            if column.style.is_some_and(|id| {
                self.styles.as_ref().map_or(id.get() != 0, |styles| {
                    styles.catalog().cell_format(id).is_none()
                })
            }) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unknown column dimension style",
                ));
            }
        }
        for range in merges {
            for style in range.appearances() {
                if self.styles.as_ref().map_or(style.get() != 0, |styles| {
                    styles.catalog().cell_format(*style).is_none()
                }) {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Unknown merged appearance identity",
                    ));
                }
            }
        }
        let maximum = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.style_memory_bytes())
            .saturating_sub(self.catalog_bytes());
        let relationships = crate::hyperlinks::relationships(hyperlinks, maximum)?;
        let maximum = maximum.saturating_sub(relationships.as_ref().map_or(0, Vec::capacity));
        let mut header = RowBuffer {
            data: Vec::new(),
            maximum,
        };
        let mut footer = RowBuffer {
            data: Vec::new(),
            maximum: 0,
        };
        let encoded = (|| -> io::Result<()> {
            header.write_all(&HEADER[..HEADER.len() - b"<sheetData>".len()])?;
            if let Some(printing) = printing {
                crate::printing::write_properties(&mut header, printing, None, true)?;
            }
            if let Some(views) = views {
                crate::worksheet_view::write_views(&mut header, views, None)?;
            }
            crate::dimension_codec::write_columns(&mut header, columns, None)?;
            header.write_all(b"<sheetData>")?;
            if printing.is_some() || !merges.is_empty() || !hyperlinks.is_empty() {
                footer.maximum = maximum.saturating_sub(header.data.capacity());
                footer.write_all(b"</sheetData>")?;
                if !merges.is_empty() {
                    write!(footer, "<mergeCells count=\"{}\">", merges.len())?;
                    for range in merges {
                        write!(footer, "<mergeCell ref=\"{}\"/>", range.range())?;
                    }
                    footer.write_all(b"</mergeCells>")?;
                }
                crate::hyperlinks::write_links(&mut footer, hyperlinks, None)?;
                if let Some(printing) = printing {
                    crate::printing::write_page(&mut footer, printing, None)?;
                    crate::printing::write_breaks(
                        &mut footer,
                        "rowBreaks",
                        &printing.row_breaks,
                        None,
                    )?;
                    crate::printing::write_breaks(
                        &mut footer,
                        "colBreaks",
                        &printing.column_breaks,
                        None,
                    )?;
                }
                footer.write_all(b"</worksheet>")?;
            }
            Ok(())
        })();
        encoded.map_err(|error| {
            Error::caused_by(
                if error.kind() == io::ErrorKind::FileTooLarge {
                    ErrorKind::LimitExceeded
                } else {
                    ErrorKind::Io
                },
                "Cannot encode worksheet metadata within allowance",
                error,
            )
        })?;
        let scratch_bytes = header
            .data
            .capacity()
            .saturating_add(footer.data.capacity())
            .saturating_add(relationships.as_ref().map_or(0, Vec::capacity));
        self.start_sheet_with_header(
            name.into(),
            &header.data,
            (printing.is_some() || !merges.is_empty() || !hyperlinks.is_empty())
                .then_some(footer.data),
            scratch_bytes,
        )?;
        if let Some(active) = &mut self.active {
            active.relationships = relationships;
        }
        Ok(())
    }
    pub(super) fn start_sheet_with_header(
        &mut self,
        name: String,
        header: &[u8],
        footer: Option<Vec<u8>>,
        scratch_bytes: usize,
    ) -> Result<()> {
        self.ensure_open()?;
        validate_sheet_name(&name)?;
        let count = self.sheets.len() + self.paused.len() + usize::from(self.active.is_some());
        if count >= self.options.max_sheets {
            return Err(limit("Writer sheet count limit exceeded"));
        }
        let folded = name.to_lowercase();
        if self
            .paused
            .iter()
            .any(|sheet| sheet.name.to_lowercase() == folded)
            || self
                .sheets
                .iter()
                .any(|sheet| sheet.name.to_lowercase() == folded)
            || self
                .active
                .as_ref()
                .is_some_and(|sheet| sheet.name.to_lowercase() == folded)
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Worksheet names must be unique ignoring case",
            ));
        }
        self.sheets
            .try_reserve_exact(count + 1 - self.sheets.len())
            .map_err(|error| {
                Error::caused_by(
                    ErrorKind::LimitExceeded,
                    "Cannot allocate writer catalog",
                    error,
                )
            })?;
        let existing = self.paused_bytes()
            + self
                .sheets
                .iter()
                .map(|sheet| sheet.name.capacity() + sheet.file.path().as_os_str().len())
                .sum::<usize>()
            + self.active.as_ref().map_or(0, |sheet| {
                sheet.name.capacity()
                    + sheet.output.get_ref().path().as_os_str().len()
                    + sheet.footer.as_ref().map_or(0, Vec::capacity)
                    + sheet.dimensions.heap_bytes()
            });
        if self
            .style_bytes()
            .saturating_add(existing)
            .saturating_add(scratch_bytes)
            .saturating_add(name.capacity())
            .saturating_add(self.sheets.capacity() * size_of::<StoredSheet>())
            > self.options.max_metadata_bytes
        {
            return Err(limit("Writer catalog budget exceeded"));
        }
        let mut builder = tempfile::Builder::new();
        builder.prefix("crabxl-");
        let file = match &self.options.temp_directory {
            Some(directory) => builder.tempfile_in(directory),
            None => builder.tempfile(),
        }
        .map_err(|error| io_error("Cannot create worksheet temporary file", error))?;
        if self
            .style_bytes()
            .saturating_add(existing)
            .saturating_add(scratch_bytes)
            .saturating_add(name.capacity())
            .saturating_add(file.path().as_os_str().len())
            .saturating_add(self.sheets.capacity() * size_of::<StoredSheet>())
            > self.options.max_metadata_bytes
        {
            return Err(limit("Writer catalog budget exceeded"));
        }
        // Reserve both the old sheet's footer and new sheet before mutating state.
        let old_footer = self.paused_footers()
            + self.active.as_ref().map_or(0, |sheet| {
                sheet.footer.as_ref().map_or(FOOTER.len(), Vec::len)
            }) as u64;
        let footer_bytes = footer.as_ref().map_or(FOOTER.len(), Vec::len) as u64;
        self.check_temp(header.len() as u64 + old_footer + footer_bytes)?;
        if header.len() as u64 + footer_bytes > self.options.max_sheet_bytes {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        self.close_sheet()?;
        self.active = Some(ActiveSheet {
            id: self.next_sheet,
            name,
            output: BufWriter::with_capacity(self.options.buffer_bytes, file),
            bytes: 0,
            last_row: None,
            dimensions: crabxl_core::SheetDimensions::default(),
            header_prefix_bytes: header.len().saturating_sub(b"<sheetData>".len()) as u64,
            columns_written: false,
            footer,
            relationships: None,
            link_spool: None,
            live_events: None,
            visibility: crabxl_core::SheetVisibility::Visible,
        });
        self.write_active(header)?;
        self.next_sheet += 1;
        Ok(())
    }
}
