// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

//! Interleaved operations for the single workbook writer owner.
use super::*;

impl WorkbookWriter {
    /// Close an empty interleaved slot as a chartsheet while preserving its tab order.
    pub fn close_interleaved_chartsheet(
        &mut self,
        id: usize,
        charts: Vec<crabxl_core::Chart>,
    ) -> Result<()> {
        self.close_interleaved_chartsheet_with_properties(id, charts, None)
    }

    /// Finish a chart-only sheet with its complete property document.
    pub fn close_interleaved_chartsheet_with_properties(
        &mut self,
        id: usize,
        charts: Vec<crabxl_core::Chart>,
        properties: Option<crabxl_core::XmlElement>,
    ) -> Result<()> {
        self.activate_sheet(id)?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.last_row.is_some()
            || active.bytes != HEADER.len() as u64
            || active.footer.is_some()
            || active.features.is_some()
            || active.relationships.is_some()
            || active.live_events.is_some()
            || active.link_spool.is_some()
            || active.dimensions.heap_bytes() != 0
        {
            return Err(state("Chartsheet requires an empty interleaved sheet"));
        }
        let features = crabxl_core::SheetFeatures {
            charts: Some(charts),
            chartsheet: properties,
            ..Default::default()
        };
        let super::sheet::PreparedChartsheet {
            header,
            footer,
            relationships,
            output,
            numbers,
        } = self.prepare_chartsheet(id, &features)?;
        let retained = footer
            .capacity()
            .saturating_add(relationships.capacity())
            .saturating_add(output.heap_bytes());
        if self
            .style_memory_bytes()
            .saturating_add(self.catalog_bytes())
            .saturating_add(retained)
            .saturating_add(header.capacity())
            > self.options.max_metadata_bytes
        {
            return Err(limit("Chartsheet metadata exceeds writer allowance"));
        }
        let bytes = header.len() as u64;
        if bytes.saturating_add(footer.len() as u64) > self.options.max_sheet_bytes {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        self.check_temp(
            bytes
                .saturating_sub(active.bytes)
                .saturating_add(footer.len() as u64)
                .saturating_add(self.paused_footers()),
        )?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        let rewrite = (|| -> io::Result<()> {
            active.output.seek(std::io::SeekFrom::Start(0))?;
            active.output.write_all(&header)?;
            active.output.flush()?;
            active.output.get_ref().as_file().set_len(bytes)
        })();
        if let Err(error) = rewrite {
            self.poisoned = true;
            return Err(io_error("Cannot spool chartsheet", error));
        }
        self.temporary_bytes = self
            .temporary_bytes
            .saturating_sub(active.bytes)
            .saturating_add(bytes);
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        active.bytes = bytes;
        active.columns_written = true;
        active.footer = Some(footer);
        active.relationships = Some(relationships);
        active.features = Some(Box::new(output));
        self.feature_numbers = numbers;
        self.close_sheet()?;
        if let Some(sheet) = self.sheets.last_mut() {
            sheet.kind = crate::SheetKind::ChartSheet;
        }
        Ok(())
    }

    /// Attach drawings after streaming rows and before closing an interleaved sheet.
    pub fn set_interleaved_drawings(
        &mut self,
        id: usize,
        images: Vec<crabxl_core::Image>,
        charts: Vec<crabxl_core::Chart>,
    ) -> Result<()> {
        if images.is_empty() && charts.is_empty() {
            self.activate_sheet(id)?;
            return Ok(());
        }
        self.set_interleaved_features(
            id,
            crabxl_core::SheetFeatures {
                images: Some(images),
                charts: Some(charts),
                ..Default::default()
            },
        )
    }

    /// Attach package-backed worksheet features before its footer is closed.
    pub fn set_interleaved_features(
        &mut self,
        id: usize,
        features: crabxl_core::SheetFeatures,
    ) -> Result<()> {
        self.set_interleaved_tail(id, features, None, &[])
    }

    /// Replace sheet properties, views and the sheet format of a live sheet.
    /// These precede the cells, so no row may have been written yet.
    pub fn set_interleaved_header(
        &mut self,
        id: usize,
        views: Option<&crabxl_core::SheetViews>,
        printing: Option<&crabxl_core::PrintSettings>,
    ) -> Result<()> {
        self.activate_sheet(id)?;
        if let Some(printing) = printing {
            crate::printing::validate(printing)?;
        }
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.last_row.is_some() || active.columns_written {
            return Err(state(
                "Worksheet views and properties must be set before rows are written",
            ));
        }
        let mut header = Vec::new();
        (|| -> io::Result<()> {
            header
                .write_all(&HEADER[..HEADER.len() - SHEET_FORMAT.len() - b"<sheetData>".len()])?;
            if let Some(printing) = printing {
                crate::printing::write_properties(&mut header, printing, None)?;
            }
            if let Some(views) = views {
                crate::worksheet_view::write_views(&mut header, views, None)?;
            }
            crate::printing::write_format(&mut header, printing, None)?;
            header.write_all(b"<sheetData>")
        })()
        .map_err(|cause| io_error("Cannot encode worksheet header", cause))?;
        let bytes = header.len() as u64;
        let footer = active.footer.as_ref().map_or(FOOTER.len(), Vec::len) as u64;
        if bytes.saturating_add(footer) > self.options.max_sheet_bytes {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        self.check_temp(
            bytes
                .saturating_sub(active.bytes)
                .saturating_add(footer)
                .saturating_add(self.paused_footers()),
        )?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        let rewrite = (|| -> io::Result<()> {
            active.output.seek(std::io::SeekFrom::Start(0))?;
            active.output.write_all(&header)?;
            active.output.flush()?;
            active.output.get_ref().as_file().set_len(bytes)
        })();
        if let Err(error) = rewrite {
            self.poisoned = true;
            return Err(io_error("Cannot spool worksheet header", error));
        }
        self.temporary_bytes = self
            .temporary_bytes
            .saturating_sub(active.bytes)
            .saturating_add(bytes);
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        active.bytes = bytes;
        active.header_prefix_bytes = bytes - b"<sheetData>".len() as u64;
        Ok(())
    }

    /// Attach features, printing settings and merged ranges, which all follow the cells.
    pub fn set_interleaved_tail(
        &mut self,
        id: usize,
        mut features: crabxl_core::SheetFeatures,
        printing: Option<&crabxl_core::PrintSettings>,
        merges: &[crabxl_core::CellRange],
    ) -> Result<()> {
        self.activate_sheet(id)?;
        if let Some(printing) = printing {
            crate::printing::validate(printing)?;
        }
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        let comment_heap = std::mem::take(&mut active.comment_heap);
        let explicit = features.comments.len();
        if explicit == 0 {
            std::mem::swap(&mut features.comments, &mut active.comments);
        } else {
            features.comments.append(&mut active.comments);
        }
        let result = self.prepare_interleaved_features(id, &features, printing, merges);
        if result.is_err() {
            features.comments.drain(..explicit);
            let active = self
                .active
                .as_mut()
                .ok_or_else(|| state("No active worksheet"))?;
            if explicit == 0 {
                active.comments = features.comments;
            } else {
                active.comments.extend(features.comments);
            }
            active.comment_heap = comment_heap;
        }
        result
    }
    /// Append a bounded row and snapshot its notes only after the row succeeds.
    pub fn write_row_with_annotations(
        &mut self,
        row: &Row,
        links: &[(crabxl_core::CellAddress, u64)],
        comments: Vec<crabxl_core::features::Comment>,
    ) -> Result<()> {
        if comments.is_empty() {
            return self.write_row_with_hyperlink_groups(row, links);
        }
        self.ensure_open()?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.features.is_some() || active.footer.is_some() {
            return Err(state(
                "Cannot append notes after preparing worksheet features",
            ));
        }
        let mut heap = 0usize;
        let mut previous = None;
        for comment in &comments {
            if comment.address.row != row.index
                || previous.is_some_and(|column| comment.address.column <= column)
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Comments must belong to the appended row in column order",
                ));
            }
            validate_xml_text(&comment.text)?;
            validate_xml_text(&comment.author)?;
            if !comment.width.is_finite()
                || !comment.height.is_finite()
                || comment.width < 0.0
                || comment.height < 0.0
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Invalid comment dimensions",
                ));
            }
            heap = heap.saturating_add(comment.heap_bytes());
            previous = Some(comment.address.column);
        }
        let length = active.comments.len().saturating_add(comments.len());
        let capacity = active.comments.capacity();
        let target = if length > capacity {
            length.max(capacity.saturating_mul(2))
        } else {
            capacity
        };
        let available = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.style_memory_bytes())
            .saturating_sub(self.catalog_bytes());
        let scratch = comments
            .capacity()
            .saturating_mul(size_of::<crabxl_core::features::Comment>())
            .saturating_add(heap);
        let growth = target
            .saturating_sub(capacity)
            .saturating_mul(size_of::<crabxl_core::features::Comment>());
        if growth.saturating_add(heap).saturating_add(scratch) > available {
            return Err(limit("Streamed comments exceed metadata allowance"));
        }
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        active
            .comments
            .try_reserve_exact(target.saturating_sub(active.comments.len()))
            .map_err(|_| limit("Cannot reserve streamed comments"))?;
        // Reserve note ownership while row/style/link validation shares the same budget.
        active.comment_heap = active
            .comment_heap
            .saturating_add(heap)
            .saturating_add(scratch);
        let result = self.write_row_with_hyperlink_groups(row, links);
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        active.comment_heap = active.comment_heap.saturating_sub(scratch);
        if result.is_ok() {
            active.comments.extend(comments);
        } else {
            active.comment_heap = active.comment_heap.saturating_sub(heap);
        }
        result
    }
    fn prepare_interleaved_features(
        &mut self,
        id: usize,
        features: &crabxl_core::SheetFeatures,
        printing: Option<&crabxl_core::PrintSettings>,
        merges: &[crabxl_core::CellRange],
    ) -> Result<()> {
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.footer.is_some() || active.relationships.is_some() || active.features.is_some() {
            return Err(state("Worksheet already has prepared features"));
        }
        if self
            .style_memory_bytes()
            .saturating_add(self.catalog_bytes())
            .saturating_add(features.heap_bytes())
            > self.options.max_metadata_bytes
        {
            return Err(limit("Streamed features exceed metadata allowance"));
        }
        let mut numbers = self.feature_numbers.clone();
        let mut feature_id = 0;
        let output = crate::features::package::build_sheet_output(
            features,
            id,
            "xl/worksheets/sheet.xml",
            &mut numbers,
            |kind| {
                if kind == "drawing" {
                    "rIdDrawing".into()
                } else {
                    feature_id += 1;
                    format!("rIdFeature{feature_id}")
                }
            },
        )
        .map_err(|error| io_error("Cannot encode streamed drawings", error))?;
        let mut footer = b"</sheetData>".to_vec();
        (|| -> io::Result<()> {
            use crate::features::sheet::{after_printing_rank, rank, write_ranks};
            let refs = &output.refs;
            let merge_rank = rank(b"mergeCells");
            write_ranks(&mut footer, features, refs, 0, merge_rank, None)?;
            if !merges.is_empty() {
                write!(footer, "<mergeCells count=\"{}\">", merges.len())?;
                for range in merges {
                    write!(footer, "<mergeCell ref=\"{range}\"/>")?;
                }
                footer.write_all(b"</mergeCells>")?;
            }
            // Streamed hyperlinks are inserted later before the first element ranked after them.
            write_ranks(
                &mut footer,
                features,
                refs,
                merge_rank + 1,
                rank(b"hyperlinks"),
                None,
            )?;
            if let Some(printing) = printing {
                crate::printing::write_page(&mut footer, printing, None)?;
            }
            let header_footer = rank(b"headerFooter");
            write_ranks(
                &mut footer,
                features,
                refs,
                header_footer,
                header_footer + 1,
                None,
            )?;
            if let Some(printing) = printing {
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
            write_ranks(
                &mut footer,
                features,
                refs,
                after_printing_rank(),
                usize::MAX,
                None,
            )?;
            footer.write_all(b"</worksheet>")
        })()
        .map_err(|error| io_error("Cannot encode streamed worksheet footer", error))?;
        let mut relationships = b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">".to_vec();
        relationships.extend_from_slice(&output.extra_relationships);
        for relationship in &output.relationships {
            relationship
                .write(&mut relationships)
                .map_err(|error| io_error("Cannot encode streamed drawing relationships", error))?;
        }
        relationships.extend_from_slice(b"</Relationships>");
        crate::features::package::validate_relationship_ids(&relationships)?;
        if self
            .style_memory_bytes()
            .saturating_add(self.catalog_bytes())
            .saturating_add(features.heap_bytes())
            .saturating_add(output.heap_bytes())
            .saturating_add(footer.capacity())
            .saturating_add(relationships.capacity())
            > self.options.max_metadata_bytes
        {
            return Err(limit("Streamed drawings exceed metadata allowance"));
        }
        if active.bytes.saturating_add(footer.len() as u64) > self.options.max_sheet_bytes {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        self.check_temp(self.paused_footers().saturating_add(footer.len() as u64))?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        active.footer = Some(footer);
        active.relationships = Some(relationships);
        active.features = Some(Box::new(output));
        self.feature_numbers = numbers;
        Ok(())
    }

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
