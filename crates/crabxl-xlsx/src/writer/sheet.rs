// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

//! Sheet operations for the single workbook writer owner.
use super::*;

pub(super) struct PreparedChartsheet {
    pub(super) header: Vec<u8>,
    pub(super) footer: Vec<u8>,
    pub(super) relationships: Vec<u8>,
    pub(super) output: crate::features::package::SheetOutput,
    pub(super) numbers: crate::features::package::PartNumbers,
}

impl WorkbookWriter {
    /// Write a complete chartsheet containing one chart, without cell materialization.
    pub fn write_chartsheet(
        &mut self,
        name: impl Into<String>,
        chart: &crabxl_core::Chart,
    ) -> Result<usize> {
        self.write_chartsheet_with_properties(name, chart, None)
    }
    /// Create a chart-only sheet with editable view, page, protection and extension properties.
    pub fn write_chartsheet_with_properties(
        &mut self,
        name: impl Into<String>,
        chart: &crabxl_core::Chart,
        properties: Option<&crabxl_core::XmlElement>,
    ) -> Result<usize> {
        let features = crabxl_core::SheetFeatures {
            charts: Some(vec![chart.clone()]),
            chartsheet: properties.cloned(),
            ..Default::default()
        };
        self.write_chartsheet_with_features(name, &features)
    }
    /// Create a chart-only sheet with drawing objects, editable properties and owned part graphs.
    pub fn write_chartsheet_with_features(
        &mut self,
        name: impl Into<String>,
        features: &crabxl_core::SheetFeatures,
    ) -> Result<usize> {
        self.ensure_open()?;
        let name = name.into();
        validate_sheet_name(&name)?;
        let id = self.next_sheet;
        let PreparedChartsheet {
            header,
            footer,
            relationships,
            output,
            numbers,
        } = self.prepare_chartsheet(id, features)?;
        self.start_sheet_with_header(
            name,
            &header,
            Some(footer),
            output
                .heap_bytes()
                .saturating_add(relationships.len())
                .saturating_add(header.capacity()),
        )?;
        if let Some(active) = &mut self.active {
            active.columns_written = true;
            active.features = Some(Box::new(output));
            active.relationships = Some(relationships);
        }
        self.feature_numbers = numbers;
        self.close_sheet()?;
        if let Some(sheet) = self.sheets.last_mut() {
            sheet.kind = crate::SheetKind::ChartSheet;
        }
        Ok(id)
    }
    pub(super) fn prepare_chartsheet(
        &self,
        id: usize,
        features: &crabxl_core::SheetFeatures,
    ) -> Result<PreparedChartsheet> {
        let properties = features.chartsheet.as_ref();
        if let Some(properties) = properties {
            crate::drawing::validate_chartsheet_copy(
                properties,
                features.chartsheet_parts.as_ref(),
            )?;
        }
        let mut inline = features.clone();
        inline.chartsheet = None;
        inline.chartsheet_parts = None;
        inline.images = None;
        inline.charts = None;
        inline.shapes = None;
        if !inline.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Edit chartsheet metadata through its XML property tree",
            ));
        }
        let mut numbers = self.feature_numbers.clone();
        let output = crate::features::package::build_sheet_output(
            features,
            id,
            "xl/chartsheets/sheet.xml",
            &mut numbers,
            |_| "rIdDrawing".into(),
        )
        .map_err(|error| io_error("Cannot encode chartsheet drawing", error))?;
        let mut relationships = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">".to_vec();
        for rel in &output.relationships {
            rel.write(&mut relationships)
                .map_err(|error| io_error("Cannot encode chartsheet relationships", error))?;
        }
        relationships.extend_from_slice(&output.extra_relationships);
        relationships.extend_from_slice(b"</Relationships>");
        let mut bytes = RowBuffer {
            data: Vec::new(),
            maximum: self.options.max_metadata_bytes,
        };
        crate::drawing::write_chartsheet(
            &mut bytes,
            properties.cloned(),
            output.refs.drawing.as_deref(),
            MAIN,
        )
        .map_err(|error| io_error("Cannot encode chartsheet properties", error))?;
        let closing = format!(
            "</{}>",
            properties.map_or("chartsheet", |properties| properties.name.as_ref())
        );
        let split = bytes
            .data
            .len()
            .checked_sub(closing.len())
            .ok_or_else(|| state("Invalid chartsheet closing tag"))?;
        let footer = bytes.data.split_off(split);
        Ok(PreparedChartsheet {
            header: bytes.data,
            footer,
            relationships,
            output,
            numbers,
        })
    }
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
    /// Borrow canonical views and printing settings. Printer payloads can be
    /// supplied with the other features through `start_sheet_with_metadata`.
    pub fn start_sheet_with_settings(
        &mut self,
        name: impl Into<String>,
        views: Option<&crabxl_core::SheetViews>,
        printing: Option<&crabxl_core::PrintSettings>,
    ) -> Result<()> {
        self.start_sheet_with_dimensions(name, views, printing, &[], &[], &Default::default(), None)
    }
    /// Start a sheet with tables, filters, rules, comments, protection and
    /// header/footer. Feature parts are packaged when the writer finishes.
    pub fn start_sheet_with_features(
        &mut self,
        name: impl Into<String>,
        features: &crabxl_core::SheetFeatures,
    ) -> Result<()> {
        self.start_sheet_with_dimensions(
            name,
            None,
            None,
            &[],
            &[],
            &Default::default(),
            Some(features),
        )
    }
    /// Start streaming cells with views, printing, point links and package-backed features.
    /// Metadata is borrowed and encoded before the previous sheet is closed.
    pub fn start_sheet_with_metadata(
        &mut self,
        name: impl Into<String>,
        views: Option<&crabxl_core::SheetViews>,
        printing: Option<&crabxl_core::PrintSettings>,
        hyperlinks: &crabxl_core::Hyperlinks,
        features: Option<&crabxl_core::SheetFeatures>,
    ) -> Result<()> {
        self.start_sheet_with_dimensions(name, views, printing, &[], &[], hyperlinks, features)
    }
    /// Borrow workbook-level metadata for binding-side property views.
    pub fn workbook_features(&self) -> Option<&crabxl_core::WorkbookFeatures> {
        self.workbook_features.as_deref()
    }
    /// Set defined names, workbook protection and document properties.
    pub fn set_workbook_features(&mut self, features: Option<crabxl_core::WorkbookFeatures>) {
        self.metadata_counts = features
            .as_ref()
            .and_then(|features| features.metadata.as_deref())
            .and_then(|roots| {
                roots
                    .iter()
                    .find_map(crabxl_core::features::WorkbookMetadata::annotation_counts)
            });
        self.workbook_features = features
            .filter(|features| !features.is_empty())
            .map(Box::new);
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_sheet_with_dimensions(
        &mut self,
        name: impl Into<String>,
        views: Option<&crabxl_core::SheetViews>,
        printing: Option<&crabxl_core::PrintSettings>,
        columns: &[crabxl_core::ColumnDimension],
        merges: &[crabxl_core::MergedCellRange],
        hyperlinks: &crabxl_core::Hyperlinks,
        features: Option<&crabxl_core::SheetFeatures>,
    ) -> Result<()> {
        self.ensure_open()?;
        let features = features.filter(|features| !features.is_empty());
        if features.is_some_and(|features| features.chartsheet.is_some()) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Use the chartsheet writer for chartsheet properties",
            ));
        }
        if views.is_none()
            && printing.is_none()
            && columns.is_empty()
            && merges.is_empty()
            && hyperlinks.is_empty()
            && features.is_none()
        {
            return self.start_sheet(name);
        }
        if let Some(views) = views {
            crate::worksheet_view::validate(views)?;
        }
        if let Some(printing) = printing {
            crate::printing::validate(printing)?;
        }
        if printing.and_then(|printing| printing.setup.printer_relationship.as_deref())
            != features
                .and_then(|features| features.printer_settings.as_ref())
                .map(|settings| settings.relationship.as_ref())
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Printer settings must match the page-setup relationship",
            ));
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
        let mut relationships = crate::hyperlinks::relationships(hyperlinks, maximum)?;
        let mut numbers = self.feature_numbers.clone();
        let feature_output = match features {
            Some(features) => {
                let mut next = 0u32;
                let output = crate::features::package::build_sheet_output(
                    features,
                    self.next_sheet,
                    "xl/worksheets/sheet.xml",
                    &mut numbers,
                    |_| {
                        loop {
                            next += 1;
                            let id = format!("rIdF{next}");
                            if features
                                .printer_settings
                                .as_ref()
                                .is_none_or(|settings| settings.relationship.as_ref() != id)
                            {
                                break id;
                            }
                        }
                    },
                )
                .map_err(|error| io_error("Cannot encode worksheet feature parts", error))?;
                if output.heap_bytes() > maximum {
                    return Err(limit("Worksheet feature parts exceed metadata allowance"));
                }
                if !output.relationships.is_empty() || !output.extra_relationships.is_empty() {
                    let mut bytes = relationships.take().unwrap_or_else(|| {
                        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>".to_vec()
                    });
                    let end = b"</Relationships>";
                    let at = bytes
                        .windows(end.len())
                        .rposition(|window| window == end)
                        .ok_or_else(|| state("Invalid worksheet relationships"))?;
                    let mut tail = output.extra_relationships.clone();
                    for relationship in &output.relationships {
                        relationship
                            .write(&mut tail)
                            .map_err(|error| io_error("Cannot encode relationships", error))?;
                    }
                    bytes.splice(at..at, tail);
                    relationships = Some(bytes);
                }
                Some(Box::new(output))
            }
            None => None,
        };
        if let Some(bytes) = &relationships {
            crate::features::package::validate_relationship_ids(bytes)?;
        }
        let maximum = maximum
            .saturating_sub(relationships.as_ref().map_or(0, Vec::capacity))
            .saturating_sub(
                feature_output
                    .as_ref()
                    .map_or(0, |output| output.heap_bytes()),
            );
        let empty_features = crabxl_core::SheetFeatures::default();
        let empty_refs = crate::features::sheet::PartRefs::default();
        let (sheet_features, refs) = match (features, &feature_output) {
            (Some(features), Some(output)) => (features, &output.refs),
            _ => (&empty_features, &empty_refs),
        };
        let mut header = RowBuffer {
            data: Vec::new(),
            maximum,
        };
        let mut footer = RowBuffer {
            data: Vec::new(),
            maximum: 0,
        };
        let encoded = (|| -> io::Result<()> {
            header
                .write_all(&HEADER[..HEADER.len() - SHEET_FORMAT.len() - b"<sheetData>".len()])?;
            if let Some(printing) = printing {
                crate::printing::write_properties(&mut header, printing, None)?;
            }
            if let Some(views) = views {
                crate::worksheet_view::write_views(&mut header, views, None)?;
            }
            crate::printing::write_format(&mut header, printing, None)?;
            crate::dimension_codec::write_columns(&mut header, columns, None)?;
            header.write_all(b"<sheetData>")?;
            if printing.is_some()
                || !merges.is_empty()
                || !hyperlinks.is_empty()
                || features.is_some()
            {
                use crate::features::sheet::{after_printing_rank, rank, write_ranks};
                footer.maximum = maximum.saturating_sub(header.data.capacity());
                footer.write_all(b"</sheetData>")?;
                let merge_rank = rank(b"mergeCells");
                write_ranks(&mut footer, sheet_features, refs, 0, merge_rank, None)?;
                if !merges.is_empty() {
                    write!(footer, "<mergeCells count=\"{}\">", merges.len())?;
                    for range in merges {
                        write!(footer, "<mergeCell ref=\"{}\"/>", range.range())?;
                    }
                    footer.write_all(b"</mergeCells>")?;
                }
                write_ranks(
                    &mut footer,
                    sheet_features,
                    refs,
                    merge_rank + 1,
                    rank(b"hyperlinks"),
                    None,
                )?;
                crate::hyperlinks::write_links(&mut footer, hyperlinks, None)?;
                if let Some(printing) = printing {
                    crate::printing::write_page(&mut footer, printing, None)?;
                }
                write_ranks(
                    &mut footer,
                    sheet_features,
                    refs,
                    rank(b"headerFooter"),
                    rank(b"headerFooter") + 1,
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
                    sheet_features,
                    refs,
                    after_printing_rank(),
                    usize::MAX,
                    None,
                )?;
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
            (printing.is_some()
                || !merges.is_empty()
                || !hyperlinks.is_empty()
                || features.is_some())
            .then_some(footer.data),
            scratch_bytes,
        )?;
        if let Some(active) = &mut self.active {
            active.relationships = relationships;
            active.features = feature_output;
        }
        self.feature_numbers = numbers;
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
            comments: Vec::new(),
            comment_heap: 0,
            footer,
            relationships: None,
            link_spool: None,
            live_events: None,
            features: None,
            visibility: crabxl_core::SheetVisibility::Visible,
        });
        self.write_active(header)?;
        self.sheet_order.push(self.next_sheet);
        self.next_sheet += 1;
        Ok(())
    }
}
