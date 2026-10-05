// SPDX-License-Identifier: MIT
// Workbook/relationship parsing adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

use crate::{
    Rows,
    strings::{SharedStringOptions, SharedStringStats, SharedStrings},
    xml::{Scope, XmlStream, attribute, required_attribute},
};
use crabxl_core::{Error, ErrorKind, ReadOptions, ResourceLimits, Result, Row, SheetData};
use quick_xml::events::Event;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};
use zip::{ZipArchive, read::ZipFile};

/// The kind of a workbook sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SheetKind {
    /// A cell worksheet, supported by the numeric streaming reader.
    Worksheet,
    /// A chartsheet; cataloged but not readable as cell rows.
    ChartSheet,
    /// A dialog sheet; cataloged but not readable as cell rows.
    DialogSheet,
}

/// One worksheet catalog entry, without loaded worksheet cells.
#[derive(Clone, Debug)]
pub struct SheetInfo {
    name: String,
    part: String,
    kind: SheetKind,
}
impl SheetInfo {
    /// Return the unescaped display name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Return the normalized archive part path.
    pub fn part(&self) -> &str {
        &self.part
    }
    /// Return the sheet kind.
    pub fn kind(&self) -> SheetKind {
        self.kind
    }
}

struct Relationship {
    target: String,
    kind: String,
    external: bool,
}

/// Owns a seekable ZIP source and a small sheet catalog, not worksheet cells.
///
/// A row reader borrows this workbook exclusively. Dropping that row reader
/// releases its entry immediately without decompressing the remaining sheet.
/// Dropping the workbook drops its owned source; no global cleanup registry exists.
pub struct WorkbookReader<R: Read + Seek = File> {
    pub(crate) archive: ZipArchive<R>,
    sheets: Vec<SheetInfo>,
    pub(crate) limits: ResourceLimits,
    date_1904: bool,
    active_sheet: usize,
    pub(crate) workbook_part: String,
    shared_string_part: Option<String>,
    shared_strings: Option<SharedStrings>,
    shared_string_options: SharedStringOptions,
    style_part: Option<String>,
    theme_part: Option<String>,
    theme: Option<crabxl_core::Theme>,
    imported_styles: Option<crate::style_reader::ImportedStyles>,
    style_metadata_remaining: u64,
}
impl WorkbookReader<File> {
    /// Open a local XLSX file with default resource limits.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_limits(path, ResourceLimits::default())
    }
    /// Open a local XLSX file with explicit resource limits.
    pub fn open_with_limits(path: impl AsRef<Path>, limits: ResourceLimits) -> Result<Self> {
        let file = File::open(path)
            .map_err(|e| Error::caused_by(ErrorKind::Io, "Cannot open workbook", e))?;
        Self::with_limits(file, limits)
    }
}
impl<R: Read + Seek> WorkbookReader<R> {
    /// Read the declared worksheet dimension without loading cells. Missing
    /// dimensions are returned as None; callers can stream to calculate them.
    /// Stops at sheetData and does not validate unread worksheet bytes or CRC.
    pub fn worksheet_dimension(&mut self, name: &str) -> Result<Option<crabxl_core::CellRange>> {
        let sheet = self
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet not found"))?;
        if sheet.kind != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Only worksheets have cell dimensions",
            ));
        }
        let part = sheet.part.clone();
        let file = self.archive.by_name(&part).map_err(|error| {
            Error::caused_by(ErrorKind::Archive, "Cannot open worksheet", error).with_part(&part)
        })?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            self.limits.max_part_bytes,
            self.limits,
        );
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(element)
                    if frame.scope == Scope::Spreadsheet
                        && element.local_name().as_ref().as_bytes() == b"dimension"
                        && frame.depth == 2 =>
                {
                    return required_attribute(&element, b"ref")?.parse().map(Some);
                }
                Event::Start(element)
                    if frame.scope == Scope::Spreadsheet
                        && element.local_name().as_ref().as_bytes() == b"sheetData"
                        && frame.depth == 2 =>
                {
                    return Ok(None);
                }
                Event::Eof => return Ok(None),
                _ => {}
            }
        }
    }
    /// Read bounded worksheet viewport metadata without materializing cells.
    /// Stops at the views container or sheetData: the unconsumed payload and CRC
    /// are not validated. Original-package save can validate every affected part.
    pub fn sheet_views(&mut self, name: &str) -> Result<crabxl_core::SheetViews> {
        self.sheet_views_with_allowance(name, usize::MAX)
    }
    pub(crate) fn sheet_views_with_allowance(
        &mut self,
        name: &str,
        allowance: usize,
    ) -> Result<crabxl_core::SheetViews> {
        let info = self
            .sheets
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, "Worksheet view source not found")
            })?;
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Worksheet views require a cell worksheet",
            ));
        }
        let part = info.part().to_owned();
        let file = self.archive.by_name(&part).map_err(|error| {
            Error::caused_by(
                ErrorKind::Archive,
                "Cannot open worksheet view source",
                error,
            )
            .with_part(part.clone())
        })?;
        let maximum = usize::try_from(self.limits.max_metadata_bytes)
            .unwrap_or(usize::MAX)
            .min(allowance);
        let bytes = file
            .size()
            .min(self.limits.max_metadata_bytes)
            .min(self.limits.max_part_bytes);
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            bytes,
            self.limits,
        );
        crate::worksheet_view::read_header(&mut xml, maximum)
    }
    /// Read printing metadata through worksheet EOF/CRC without materializing cells.
    /// Full decompression is necessary because printing elements follow sheetData.
    pub fn print_settings(&mut self, name: &str) -> Result<crabxl_core::PrintSettings> {
        self.print_settings_with_allowance(name, usize::MAX)
    }
    pub(crate) fn print_settings_with_allowance(
        &mut self,
        name: &str,
        allowance: usize,
    ) -> Result<crabxl_core::PrintSettings> {
        let info = self
            .sheets
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    "Printing worksheet source not found",
                )
            })?;
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Printing metadata requires a cell worksheet",
            ));
        }
        let part = info.part().to_owned();
        let file = self.archive.by_name(&part).map_err(|error| {
            Error::caused_by(
                ErrorKind::Archive,
                "Cannot open printing worksheet source",
                error,
            )
            .with_part(part.clone())
        })?;
        let maximum = usize::try_from(self.limits.max_metadata_bytes)
            .unwrap_or(usize::MAX)
            .min(allowance);
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            self.limits.max_part_bytes,
            self.limits,
        );
        crate::printing::read(&mut xml, maximum)
    }
    /// Read package metadata from a seekable owned source with default limits.
    pub fn new(source: R) -> Result<Self> {
        Self::with_limits(source, ResourceLimits::default())
    }
    /// Read package metadata without loading cells, strings, or styles.
    pub fn with_limits(mut source: R, limits: ResourceLimits) -> Result<Self> {
        validate_limits(limits)?;
        let size = source
            .seek(SeekFrom::End(0))
            .map_err(|e| Error::caused_by(ErrorKind::Io, "Cannot measure archive", e))?;
        if size > limits.max_archive_bytes {
            return Err(limit("Compressed archive size limit exceeded"));
        }
        source
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::caused_by(ErrorKind::Io, "Cannot seek archive", e))?;
        let mut archive = ZipArchive::new(source)
            .map_err(|e| Error::caused_by(ErrorKind::Archive, "Cannot read ZIP archive", e))?;
        if archive.len() > limits.max_archive_entries {
            return Err(limit("Archive entry count limit exceeded"));
        }
        if archive
            .decompressed_size()
            .is_none_or(|n| n > u128::from(limits.max_total_uncompressed_bytes))
        {
            return Err(limit("Declared uncompressed archive size limit exceeded"));
        }
        let mut metadata_remaining = limits.max_metadata_bytes;
        let root =
            read_relationships(&mut archive, "_rels/.rels", limits, &mut metadata_remaining)?;
        let mut offices = root
            .values()
            .filter(|r| relationship_is(&r.kind, "officeDocument"));
        let office = offices
            .next()
            .ok_or_else(|| invalid("Package has no officeDocument relationship"))?;
        if offices.next().is_some() || office.external {
            return Err(invalid("Invalid officeDocument relationship"));
        }
        let workbook_part = resolve_part("", &office.target)?;
        let content_types = read_content_types(&mut archive, limits, &mut metadata_remaining)?;
        let workbook_type = content_types
            .get(&workbook_part)
            .ok_or_else(|| invalid("Workbook content type is missing"))?;
        if !matches!(
            workbook_type.as_str(),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
                | "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml"
                | "application/vnd.ms-excel.sheet.macroEnabled.main+xml"
                | "application/vnd.ms-excel.template.macroEnabled.main+xml"
        ) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Package is not an XLSX-family workbook",
            ));
        }
        let rels_part = relationship_part(&workbook_part);
        let relationships =
            read_relationships(&mut archive, &rels_part, limits, &mut metadata_remaining)?;
        let (sheets, date_1904, active_sheet) = read_workbook(
            &mut archive,
            &workbook_part,
            &relationships,
            limits,
            &mut metadata_remaining,
        )?;
        for sheet in &sheets {
            if archive.index_for_name(&sheet.part).is_none() {
                return Err(invalid("Worksheet part is missing").with_part(sheet.part.clone()));
            }
            let expected = match sheet.kind {
                SheetKind::Worksheet => {
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
                }
                SheetKind::ChartSheet => {
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml"
                }
                SheetKind::DialogSheet => {
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml"
                }
            };
            if content_types.get(&sheet.part).map(String::as_str) != Some(expected) {
                return Err(
                    invalid("Sheet content type does not match its relationship")
                        .with_part(sheet.part.clone()),
                );
            }
        }
        let mut string_rels = relationships
            .values()
            .filter(|r| relationship_is(&r.kind, "sharedStrings"));
        let shared_string_part = string_rels
            .next()
            .map(|r| {
                if r.external {
                    return Err(invalid("Shared-string relationship must be internal"));
                }
                resolve_part(&workbook_part, &r.target)
            })
            .transpose()?;
        if string_rels.next().is_some() {
            return Err(invalid("Duplicate shared-string relationships"));
        }
        let mut style_rels = relationships
            .values()
            .filter(|r| relationship_is(&r.kind, "styles"));
        let style_part = style_rels
            .next()
            .map(|r| {
                if r.external {
                    return Err(invalid("Style relationship must be internal"));
                }
                resolve_part(&workbook_part, &r.target)
            })
            .transpose()?;
        if style_rels.next().is_some() {
            return Err(invalid("Duplicate style relationships"));
        }
        let mut theme_rels = relationships
            .values()
            .filter(|r| relationship_is(&r.kind, "theme"));
        let theme_part = theme_rels
            .next()
            .map(|r| {
                if r.external {
                    return Err(invalid("Theme relationship must be internal"));
                }
                resolve_part(&workbook_part, &r.target)
            })
            .transpose()?;
        if theme_rels.next().is_some() {
            return Err(invalid("Duplicate theme relationships"));
        }
        Ok(Self {
            archive,
            sheets,
            limits,
            date_1904,
            active_sheet,
            workbook_part,
            shared_string_part,
            shared_strings: None,
            shared_string_options: SharedStringOptions::default(),
            style_part,
            theme_part,
            theme: None,
            imported_styles: None,
            style_metadata_remaining: metadata_remaining,
        })
    }
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
    /// Load and borrow the shared style catalog without materializing a worksheet.
    /// Unknown/staged root sections remain explicitly listed; original-package
    /// preservation does not imply typed support for those sections.
    pub fn style_catalog(&mut self) -> Result<Option<&crabxl_core::StyleCatalog>> {
        self.prepare_styles()?;
        Ok(self.imported_styles.as_ref().map(|s| &s.catalog))
    }
    /// Consume this reader and transfer its validated style catalog without cloning.
    /// Remaining archive/string-cache resources close when the reader is consumed.
    /// Derived date lookups are discarded; canonical format records retain their kinds.
    pub fn into_style_catalog(mut self) -> Result<Option<crabxl_core::StyleCatalog>> {
        self.prepare_styles()?;
        Ok(self.imported_styles.take().map(|styles| styles.catalog))
    }
    /// Retained style catalog plus derived number-format classifications.
    pub fn style_memory_bytes(&self) -> usize {
        self.imported_styles
            .as_ref()
            .map_or(0, |s| s.memory_bytes())
    }
    fn prepare_styles(&mut self) -> Result<()> {
        self.prepare_styles_with_allowance(None)
    }
    fn prepare_styles_with_allowance(&mut self, allowance: Option<usize>) -> Result<()> {
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
    /// Inspect workbook sheets without loading their data.
    pub fn sheets(&self) -> &[SheetInfo] {
        &self.sheets
    }
    /// First workbook view's active display position, or None if out of range.
    /// Missing view metadata defaults to the first sheet.
    pub fn active_index(&self) -> Option<usize> {
        (self.active_sheet < self.sheets.len()).then_some(self.active_sheet)
    }
    /// Whether the workbook uses the 1904 date origin.
    pub fn date_1904(&self) -> bool {
        self.date_1904
    }
    /// Explicitly load an entire sparse numeric worksheet into owned memory.
    ///
    /// Uses the same validated decoder and value restrictions as streaming.
    /// Retained vector capacities must fit `max_materialized_bytes`; one bounded
    /// current row and parser/catalog allocations are additional working memory.
    /// Useful for repeated in-memory access; first-pass decoding is not faster
    /// merely because all output is retained. Errors discard partial output.
    pub fn read_sheet(&mut self, name: &str) -> Result<SheetData> {
        self.collect_sheet(name, self.limits.max_materialized_bytes)
    }
    /// Materialize selected cells using the same rich/date/formula policies as streaming.
    /// Catalogs and one current row remain separately bounded working allocations.
    pub fn read_sheet_with_options(
        &mut self,
        name: &str,
        options: ReadOptions,
    ) -> Result<SheetData> {
        self.collect_sheet_options(name, self.limits.max_materialized_bytes, options)
    }
    pub(crate) fn collect_sheet(&mut self, name: &str, maximum: usize) -> Result<SheetData> {
        self.collect_sheet_options(name, maximum, ReadOptions::default())
    }
    fn collect_sheet_options(
        &mut self,
        name: &str,
        maximum: usize,
        options: ReadOptions,
    ) -> Result<SheetData> {
        self.collect_sheet_with_allowance(name, maximum, options, None)
    }
    pub(crate) fn collect_sheet_with_allowance(
        &mut self,
        name: &str,
        maximum: usize,
        options: ReadOptions,
        allowance: Option<usize>,
    ) -> Result<SheetData> {
        let part = self
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .map(|sheet| sheet.part.clone());
        let materialization_limit = || {
            let error = Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Materialized sheet allocation exceeds the configured budget",
            );
            match &part {
                Some(part) => error.with_part(part),
                None => error,
            }
        };
        let mut stream = self.rows_with_allowance(name, options, allowance)?;
        let mut sheet = SheetData { rows: Vec::new() };
        let mut cell_bytes = 0usize;
        while let Some(row) = stream.next_row()? {
            let row_bytes = row.memory_bytes().saturating_sub(size_of::<Row>());
            cell_bytes = cell_bytes
                .checked_add(row_bytes)
                .ok_or_else(materialization_limit)?;
            let row_allowance = maximum.min(stream.available_retained_bytes()?);
            let available_rows = row_allowance
                .saturating_sub(size_of::<SheetData>())
                .saturating_sub(cell_bytes)
                / size_of::<Row>();
            if sheet.rows.len() == sheet.rows.capacity() {
                let wanted = sheet
                    .rows
                    .capacity()
                    .saturating_mul(2)
                    .max(16)
                    .min(available_rows);
                if wanted <= sheet.rows.len() {
                    return Err(materialization_limit());
                }
                stream.set_aggregate_retained(
                    size_of::<SheetData>()
                        .saturating_add(wanted.saturating_mul(size_of::<Row>()))
                        .saturating_add(cell_bytes),
                )?;
                sheet
                    .rows
                    .try_reserve_exact(wanted - sheet.rows.len())
                    .map_err(|e| {
                        let error = Error::caused_by(
                            ErrorKind::LimitExceeded,
                            "Cannot allocate materialized sheet",
                            e,
                        );
                        match &part {
                            Some(part) => error.with_part(part),
                            None => error,
                        }
                    })?;
            }
            let retained = size_of::<SheetData>()
                .saturating_add(sheet.rows.capacity().saturating_mul(size_of::<Row>()))
                .saturating_add(cell_bytes);
            if retained > row_allowance {
                return Err(materialization_limit());
            }
            stream.set_aggregate_retained(retained)?;
            sheet.rows.push(row);
        }
        if sheet.memory_bytes() > maximum {
            return Err(materialization_limit());
        }
        Ok(sheet)
    }

    /// Stream all present numeric/empty cells in a worksheet.
    pub fn rows(&mut self, name: &str) -> Result<Rows<'_, R>> {
        self.rows_with_options(name, ReadOptions::default())
    }
    /// Stream selected sparse rows and columns, without decoding excluded cells.
    pub fn rows_with_options(&mut self, name: &str, options: ReadOptions) -> Result<Rows<'_, R>> {
        self.rows_with_allowance(name, options, None)
    }
    pub(crate) fn rows_with_allowance(
        &mut self,
        name: &str,
        options: ReadOptions,
        allowance: Option<usize>,
    ) -> Result<Rows<'_, R>> {
        if options.rows.as_ref().is_some_and(|r| r.start() > r.end())
            || options
                .columns
                .as_ref()
                .is_some_and(|r| r.start() > r.end())
        {
            return Err(invalid("Projection range is reversed"));
        }
        let sheet = self.sheets.iter().find(|s| s.name == name).ok_or_else(|| {
            Error::new(
                ErrorKind::SheetNotFound,
                format!("Worksheet not found: {name}"),
            )
        })?;
        if sheet.kind != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Only cell worksheets support row streaming",
            ));
        }
        let part = sheet.part.clone();
        let style_allowance = allowance
            .map(|maximum| {
                maximum
                    .checked_sub(
                        self.catalog_memory_bytes()
                            .saturating_sub(self.style_memory_bytes()),
                    )
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Package catalogs exceed aggregate allowance",
                        )
                    })
            })
            .transpose()?;
        self.prepare_styles_with_allowance(style_allowance)?;
        let fixed_bytes = self.catalog_memory_bytes();
        let pool = allowance
            .map(|maximum| {
                maximum
                    .checked_sub(fixed_bytes)
                    .map(|pool_bytes| crate::aggregate::ReadPool {
                        fixed_bytes,
                        pool_bytes,
                        retained_bytes: 0,
                    })
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Prepared catalogs exceed aggregate allowance",
                        )
                    })
            })
            .transpose()?;
        if options.rich_text
            && self
                .shared_strings
                .as_ref()
                .is_some_and(|s| !s.stats().rich_text_preserved)
        {
            // Rebuild once when upgrading a plain projection to metadata-preserving reads.
            self.shared_strings = None;
        }
        if self.shared_strings.is_none()
            && let Some(string_part) = &self.shared_string_part
        {
            let file = self.archive.by_name(string_part).map_err(|e| {
                Error::caused_by(ErrorKind::Archive, "Cannot open shared-string part", e)
                    .with_part(string_part)
            })?;
            if file.size() > self.limits.max_part_bytes {
                return Err(limit("Shared-string part size limit exceeded").with_part(string_part));
            }
            let mut string_options = self.shared_string_options.clone();
            if let Some(pool) = &pool {
                let details = crate::memory_allowance(string_options.memory_policy, self.limits)?;
                let retained = details.retained_data_bytes.min(pool.pool_bytes);
                string_options.memory_policy = crabxl_core::MemoryPolicy::Budget(
                    details
                        .working_reserve_bytes
                        .checked_add(retained)
                        .ok_or_else(|| invalid("Aggregate shared-string allowance overflows"))?,
                );
            }
            let strings = SharedStrings::parse(
                BufReader::with_capacity(self.limits.input_buffer_bytes, file),
                string_part.clone(),
                self.limits,
                &string_options,
                options.rich_text,
            )?;
            self.shared_strings = Some(strings);
        }
        if let (Some(pool), Some(strings)) = (&pool, &mut self.shared_strings) {
            strings.limit_or_spill(&self.shared_string_options, pool.pool_bytes)?;
        }
        let file = self.archive.by_name(&part).map_err(|e| {
            Error::caused_by(ErrorKind::Archive, "Cannot open worksheet part", e)
                .with_part(part.clone())
        })?;
        if file.size() > self.limits.max_part_bytes {
            return Err(limit("Worksheet part size limit exceeded").with_part(part));
        }
        Rows::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            self.limits,
            options,
            self.shared_strings.as_mut(),
            self.imported_styles.as_ref(),
            if self.date_1904 {
                crabxl_core::DateEpoch::Mac1904
            } else {
                crabxl_core::DateEpoch::Windows1900
            },
        )?
        .with_read_pool(pool, &self.shared_string_options)
    }
    /// Configure shared-string storage. This releases any prepared table/cache and
    /// owned temporary files; the next row stream rebuilds from the original source.
    pub fn set_shared_string_options(&mut self, options: SharedStringOptions) {
        self.shared_strings = None;
        self.shared_string_options = options;
    }
    /// Diagnostics for the prepared shared-string table, absent before first row access.
    pub fn shared_string_stats(&self) -> Option<SharedStringStats> {
        self.shared_strings.as_ref().map(SharedStrings::stats)
    }
    /// Return the original source after all borrowed readers have been released.
    pub fn into_inner(self) -> R {
        self.archive.into_inner()
    }
}

fn validate_limits(l: ResourceLimits) -> Result<()> {
    if l.input_buffer_bytes == 0
        || l.max_materialized_bytes == 0
        || l.max_archive_bytes == 0
        || l.max_archive_entries == 0
        || l.max_total_uncompressed_bytes == 0
        || l.max_part_bytes == 0
        || l.max_metadata_bytes == 0
        || l.max_xml_event_bytes == 0
        || l.max_cell_bytes == 0
        || l.max_xml_depth == 0
        || l.max_sheets == 0
        || l.max_row_cells == 0
        || l.max_row_bytes == 0
        || l.max_batch_rows == 0
        || l.max_batch_bytes == 0
    {
        return Err(invalid("Resource limits must be positive"));
    }
    Ok(())
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn limit(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::LimitExceeded, message)
}

fn metadata_xml<'a, R: Read + Seek>(
    archive: &'a mut ZipArchive<R>,
    part: &str,
    limits: ResourceLimits,
    remaining: &mut u64,
) -> Result<XmlStream<BufReader<ZipFile<'a, R>>>> {
    let file = archive.by_name(part).map_err(|e| {
        Error::caused_by(ErrorKind::Archive, "Cannot open metadata part", e).with_part(part)
    })?;
    let size = file.size();
    *remaining = remaining
        .checked_sub(size)
        .ok_or_else(|| limit("Combined metadata byte limit exceeded").with_part(part))?;
    Ok(XmlStream::new(
        BufReader::new(file),
        part.into(),
        size.min(limits.max_metadata_bytes),
        limits,
    ))
}

fn read_relationships<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    part: &str,
    limits: ResourceLimits,
    remaining: &mut u64,
) -> Result<HashMap<String, Relationship>> {
    let mut xml = metadata_xml(archive, part, limits, remaining)?;
    let mut relationships = HashMap::new();
    let mut valid_root = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                valid_root = frame.scope == Scope::Relationships
                    && e.local_name().as_ref().as_bytes() == b"Relationships";
                if !valid_root {
                    return Err(invalid("Invalid relationships root").with_part(part));
                }
            }
            Event::Start(e)
                if frame.scope == Scope::Relationships
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"Relationship" =>
            {
                if relationships.len() >= limits.max_archive_entries {
                    return Err(limit("Relationship count limit exceeded").with_part(part));
                }
                let id = required_attribute(&e, b"Id")?;
                let kind = required_attribute(&e, b"Type")?;
                let target = required_attribute(&e, b"Target")?;
                let mode = attribute(&e, b"TargetMode")?;
                if mode
                    .as_deref()
                    .is_some_and(|m| m != "Internal" && m != "External")
                {
                    return Err(invalid("Invalid relationship TargetMode").with_part(part));
                }
                if id.is_empty()
                    || relationships
                        .insert(
                            id,
                            Relationship {
                                kind,
                                target,
                                external: mode.as_deref() == Some("External"),
                            },
                        )
                        .is_some()
                {
                    return Err(invalid("Empty or duplicate relationship ID").with_part(part));
                }
            }
            Event::Eof => {
                if !valid_root {
                    return Err(invalid("Missing relationships root").with_part(part));
                }
                return Ok(relationships);
            }
            _ => {}
        }
    }
}

fn read_content_types<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: ResourceLimits,
    remaining: &mut u64,
) -> Result<HashMap<String, String>> {
    let part = "[Content_Types].xml";
    let mut xml = metadata_xml(archive, part, limits, remaining)?;
    let mut types = HashMap::new();
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == 1
                    && (frame.scope != Scope::ContentTypes
                        || e.local_name().as_ref().as_bytes() != b"Types") =>
            {
                return Err(invalid("Invalid content types root").with_part(part));
            }
            Event::Start(e)
                if frame.scope == Scope::ContentTypes
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"Override" =>
            {
                if types.len() >= limits.max_archive_entries {
                    return Err(limit("Content type count limit exceeded").with_part(part));
                }
                let name = required_attribute(&e, b"PartName")?;
                let content_type = required_attribute(&e, b"ContentType")?;
                let name = resolve_part("", &name)?;
                if types.insert(name, content_type).is_some() {
                    return Err(invalid("Duplicate content type override").with_part(part));
                }
            }
            Event::Eof => return Ok(types),
            _ => {}
        }
    }
}

fn read_workbook<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    part: &str,
    rels: &HashMap<String, Relationship>,
    limits: ResourceLimits,
    remaining: &mut u64,
) -> Result<(Vec<SheetInfo>, bool, usize)> {
    let mut xml = metadata_xml(archive, part, limits, remaining)?;
    let mut sheets = Vec::new();
    let mut names = HashSet::new();
    let mut date_1904 = false;
    let mut inside_sheets = false;
    let mut inside_views = false;
    let mut view_seen = false;
    let mut active_sheet = 0usize;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == 1
                    && (frame.scope != Scope::Spreadsheet
                        || e.local_name().as_ref().as_bytes() != b"workbook") =>
            {
                return Err(invalid("Invalid workbook root").with_part(part));
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheets" =>
            {
                inside_sheets = true;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"sheets" =>
            {
                inside_sheets = false;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"bookViews" =>
            {
                inside_views = true;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"bookViews" =>
            {
                inside_views = false;
            }
            Event::Start(e)
                if inside_views
                    && !view_seen
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"workbookView" =>
            {
                active_sheet = attribute(&e, b"activeTab")?
                    .map(|value| {
                        value.parse::<usize>().map_err(|error| {
                            Error::caused_by(
                                ErrorKind::InvalidData,
                                "Invalid active sheet index",
                                error,
                            )
                            .with_part(part)
                        })
                    })
                    .transpose()?
                    .unwrap_or(0);
                view_seen = true;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"workbookPr" =>
            {
                date_1904 = match attribute(&e, b"date1904")?.as_deref() {
                    None | Some("0" | "false") => false,
                    Some("1" | "true") => true,
                    _ => return Err(invalid("Invalid date1904 flag").with_part(part)),
                };
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && inside_sheets
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"sheet" =>
            {
                if sheets.len() >= limits.max_sheets {
                    return Err(limit("Worksheet catalog limit exceeded").with_part(part));
                }
                let name = required_attribute(&e, b"name")?;
                if name.is_empty() || !names.insert(name.clone()) {
                    return Err(invalid("Empty or duplicate sheet name").with_part(part));
                }
                let id = frame
                    .office_relationship
                    .ok_or_else(|| invalid("Sheet relationship ID is missing").with_part(part))?;
                let relationship = rels
                    .get(&id)
                    .ok_or_else(|| invalid("Sheet relationship is missing").with_part(part))?;
                if relationship.external {
                    return Err(
                        invalid("Worksheet relationship cannot be external").with_part(part)
                    );
                }
                let kind = if relationship_is(&relationship.kind, "worksheet") {
                    SheetKind::Worksheet
                } else if relationship_is(&relationship.kind, "chartsheet") {
                    SheetKind::ChartSheet
                } else if relationship_is(&relationship.kind, "dialogsheet") {
                    SheetKind::DialogSheet
                } else {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Unsupported sheet relationship type",
                    )
                    .with_part(part));
                };
                sheets.push(SheetInfo {
                    name,
                    part: resolve_part(part, &relationship.target)?,
                    kind,
                });
            }
            Event::Eof => return Ok((sheets, date_1904, active_sheet)),
            _ => {}
        }
    }
}

pub(crate) fn relationship_is(value: &str, kind: &str) -> bool {
    value.strip_prefix("http://schemas.openxmlformats.org/officeDocument/2006/relationships/")
        == Some(kind)
        || value.strip_prefix("http://purl.oclc.org/ooxml/officeDocument/relationships/")
            == Some(kind)
}
pub(crate) fn relationship_part(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}
pub(crate) fn resolve_part(source: &str, target: &str) -> Result<String> {
    let decoded = percent_encoding::percent_decode_str(target)
        .decode_utf8()
        .map_err(|e| Error::caused_by(ErrorKind::InvalidData, "Invalid part URI encoding", e))?;
    if decoded.contains(['\\', ':', '?', '#', '\0']) {
        return Err(invalid("Invalid internal part URI"));
    }
    let mut segments: Vec<&str> = if decoded.starts_with('/') {
        Vec::new()
    } else {
        source
            .rsplit_once('/')
            .map_or_else(Vec::new, |(folder, _)| folder.split('/').collect())
    };
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(invalid("Part URI escapes package root"));
                }
            }
            part => segments.push(part),
        }
    }
    if segments.is_empty() {
        return Err(invalid("Empty internal part URI"));
    }
    Ok(segments.join("/"))
}

pub(crate) fn relationship_source(part: &str) -> Option<String> {
    let (prefix, name) = if let Some(name) = part.strip_prefix("_rels/") {
        ("", name)
    } else {
        part.rsplit_once("/_rels/")?
    };
    let name = name.strip_suffix(".rels")?;
    if name.contains('/') {
        return None;
    }
    Some(if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}/{name}")
    })
}
