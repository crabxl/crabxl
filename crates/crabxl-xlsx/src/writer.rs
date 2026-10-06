// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

use crate::encode::{
    DateStyleIds, RowBuffer, StyleContext, ValueEncoding, encode_cells, validate_xml_text,
};
use crabxl_core::{
    CellStyle, DateEpoch, Error, ErrorKind, MAX_COLUMNS, Result, Row, RowIndex, StyleCatalog,
    StyleId, StyleLimits, StyleRegistry,
};
use std::{
    io::{self, BufReader, BufWriter, Seek, Write},
    path::PathBuf,
};
use tempfile::NamedTempFile;
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::xml::{MAIN_URI as MAIN, OFFICE_REL_URI as REL};
const HEADER: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>";
const FOOTER: &[u8] = b"</sheetData></worksheet>";

/// XLSX serialization of nonfinite numeric values and formula caches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NonFiniteWritePolicy {
    /// Match the public baseline: write a present blank numeric value.
    #[default]
    Blank,
    /// Reject the row before committing any temporary XML.
    Reject,
}

/// Serialization of optional formula flags and empty data-table inputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormulaWritePolicy {
    /// Omit newly assigned false flags and empty table inputs like the public baseline.
    /// Original source flag spellings are retained, including zero/false strings.
    #[default]
    Compatible,
    /// Retain explicit optional false flags and empty table inputs.
    RetainExplicit,
}

/// Serialization of alignment attributes with zero/false values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StyleWritePolicy {
    /// Match public reference omission, including zero rotations and false wrap flags.
    #[default]
    Compatible,
    /// Preserve explicit optional alignment attributes as a Rust extension.
    RetainExplicit,
}

/// Configurable resource limits for sequential worksheet spooling.
/// These bound managed buffers/metadata and temporary XML, not process RSS.
#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// ZIP level 0..=9: 0 stores without compression; 1..=9 uses Deflate.
    /// None retains the backend default (6).
    pub compression_level: Option<u8>,
    /// Owned temporary files are created here; None uses the system directory.
    pub temp_directory: Option<PathBuf>,
    /// Buffer for the one active worksheet's temporary file.
    pub buffer_bytes: usize,
    /// Maximum worksheet names and temporary files.
    pub max_sheets: usize,
    /// Managed sheet catalog/name allocation budget, excluding allocator overhead.
    pub max_metadata_bytes: usize,
    /// Maximum encoded bytes in one row buffer.
    pub max_row_bytes: usize,
    /// Maximum decoded UTF-8 bytes in one scalar payload.
    pub max_cell_bytes: usize,
    /// Maximum present cells in one sparse row.
    pub max_row_cells: usize,
    /// Maximum temporary XML bytes across all sheets, including buffered bytes.
    pub max_temp_bytes: u64,
    /// Maximum temporary XML bytes in one sheet.
    pub max_sheet_bytes: u64,
    /// Maximum shared basic cell formats, including five initial records.
    pub max_styles: usize,
    /// Select the workbook's 1904 date epoch.
    pub date_1904: bool,
    /// Encode calendar and clock values as ISO date cells; durations remain numeric.
    pub iso_dates: bool,
    /// Nonfinite serialization is compatible by default, with explicit strict rejection.
    pub non_finite: NonFiniteWritePolicy,
    /// Compatible formula omission or explicit attribute retention.
    pub formula_attributes: FormulaWritePolicy,
    /// Compatible alignment omission or explicit attribute retention.
    pub style_attributes: StyleWritePolicy,
    /// Reference default, opaque/validated custom bytes, or explicit omission.
    pub theme: crate::ThemeWritePolicy,
    /// Zero-based active display sheet, checked against the completed catalog.
    pub active_sheet: usize,
}
impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            compression_level: None,
            temp_directory: None,
            buffer_bytes: 32768,
            max_sheets: 1024,
            max_metadata_bytes: 16 * 1024 * 1024,
            max_row_bytes: 1024 * 1024,
            max_cell_bytes: 64 * 1024,
            max_row_cells: MAX_COLUMNS as usize,
            max_temp_bytes: 4 * 1024 * 1024 * 1024,
            max_sheet_bytes: 2 * 1024 * 1024 * 1024,
            date_1904: false,
            iso_dates: false,
            non_finite: NonFiniteWritePolicy::default(),
            formula_attributes: FormulaWritePolicy::default(),
            style_attributes: StyleWritePolicy::default(),
            theme: crate::ThemeWritePolicy::default(),
            active_sheet: 0,
            max_styles: 8192,
        }
    }
}

/// Progress including logical temporary bytes, which also count unflushed XML.
#[derive(Clone, Copy, Debug, Default)]
pub struct WriteStats {
    /// Successfully spooled rows.
    pub rows: u64,
    /// Successfully spooled cells.
    pub cells: u64,
    /// Maximum temporary worksheet bytes retained during this writer's lifetime.
    pub peak_temp_bytes: u64,
}
struct StoredSheet {
    id: usize,
    name: String,
    file: NamedTempFile,
    visibility: crabxl_core::SheetVisibility,
}
struct ActiveSheet {
    id: usize,
    name: String,
    output: BufWriter<NamedTempFile>,
    bytes: u64,
    last_row: Option<RowIndex>,
    footer: Option<Vec<u8>>,
    visibility: crabxl_core::SheetVisibility,
}

/// One-shot XLSX writer using bounded rows and owned temporary worksheets.
///
/// Start sheets sequentially and write increasing row indices. Earlier sheets
/// and rows cannot be edited. Finish explicitly packages files into a seekable
/// output. Abort and Drop remove owned temporary files without publishing a
/// workbook. Caller-owned sinks can be passed to finish as &mut W.
pub struct WorkbookWriter {
    options: WriteOptions,
    view_index: Option<i64>,
    sheets: Vec<StoredSheet>,
    styles: Option<StyleRegistry>,
    canonical_styles: bool,
    date_styles: DateStyleIds,
    active: Option<ActiveSheet>,
    paused: Vec<ActiveSheet>,
    next_sheet: usize,
    row_buffer: RowBuffer,
    temporary_bytes: u64,
    stats: WriteStats,
    aborted: bool,
    poisoned: bool,
    cleanup_paths: Vec<PathBuf>,
}
// Constructor-only ownership transfer: keep this bounded stack value unboxed
// rather than adding a heap allocation that is immediately moved into the writer.
#[allow(clippy::large_enum_variant)]
enum StyleSource {
    Default,
    Catalog(StyleCatalog),
    Registry(StyleRegistry),
}
impl WorkbookWriter {
    /// Create a writer with explicit resource and temporary-directory options.
    pub fn new(options: WriteOptions) -> Result<Self> {
        Self::new_with_styles(options, StyleSource::Default)
    }
    /// Adopt source tables and preserve their component/format IDs in a new package.
    /// Automatic date formats register after existing records rather than using fixed IDs.
    /// Unmodeled extensions require original-package preservation and are rejected here.
    pub fn from_style_catalog(options: WriteOptions, catalog: StyleCatalog) -> Result<Self> {
        Self::new_with_styles(options, StyleSource::Catalog(catalog))
    }
    /// Consume an owned workbook into sequential output without cloning its styles
    /// or cells. Original-package preservation remains a separate editor operation.
    pub fn from_workbook(
        mut options: WriteOptions,
        workbook: crabxl_core::Workbook,
    ) -> Result<Self> {
        let parts = workbook.into_parts();
        options.active_sheet = parts
            .active_sheet
            .ok_or_else(|| state("A workbook requires at least one worksheet"))?;
        options.date_1904 = parts.epoch == DateEpoch::Mac1904;
        if let Some(theme) = parts.theme {
            options.theme = if matches!(options.theme, crate::ThemeWritePolicy::Validated(_)) {
                crate::ThemeWritePolicy::Validated(theme)
            } else {
                crate::ThemeWritePolicy::Custom(theme)
            };
        }
        let canonical_styles = parts.styles.is_some();
        let source = parts
            .styles
            .map_or(StyleSource::Default, StyleSource::Registry);
        let mut writer = Self::new_with_styles(options, source)?;
        writer.canonical_styles = canonical_styles;
        for sheet in parts.sheets {
            writer.write_worksheet(&sheet)?;
        }
        writer.canonical_styles = false;
        Ok(writer)
    }
    fn new_with_styles(options: WriteOptions, source: StyleSource) -> Result<Self> {
        validate_compression_level(options.compression_level)?;
        if options.max_styles < 5
            || options.max_styles > u32::MAX as usize
            || options.buffer_bytes == 0
            || options.max_sheets == 0
            || options.max_metadata_bytes == 0
            || options.max_row_bytes == 0
            || options.max_cell_bytes == 0
            || options.max_row_cells == 0
            || options.max_temp_bytes == 0
            || options.max_sheet_bytes == 0
            || options.buffer_bytes > options.max_metadata_bytes
            || options.buffer_bytes > isize::MAX as usize
            || options.max_row_bytes > isize::MAX as usize
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Writer resource limits are invalid",
            ));
        }
        if options.theme.memory_bytes() > options.max_metadata_bytes {
            return Err(limit("Writer theme metadata budget exceeded"));
        }
        if let crate::ThemeWritePolicy::Validated(theme) = &options.theme {
            crate::theme::validate(
                theme.bytes(),
                "xl/theme/theme1.xml",
                crabxl_core::ResourceLimits {
                    max_theme_bytes: options.max_metadata_bytes,
                    max_part_bytes: options.max_metadata_bytes as u64,
                    ..Default::default()
                },
            )?;
        }
        let row_buffer = RowBuffer {
            data: Vec::new(),
            maximum: options.max_row_bytes,
        };
        let style_limits = StyleLimits {
            max_bytes: options
                .max_metadata_bytes
                .saturating_sub(options.theme.memory_bytes()),
            max_records: options.max_styles,
        };
        let mut styles = match source {
            StyleSource::Default => StyleRegistry::new(style_limits).map_err(writer_style_error)?,
            StyleSource::Catalog(catalog) => {
                crate::styles::validate_catalog(&catalog)?;
                StyleRegistry::from_catalog(catalog, style_limits).map_err(writer_style_error)?
            }
            StyleSource::Registry(mut registry) => {
                crate::styles::validate_catalog(registry.catalog())?;
                registry
                    .set_limits(style_limits)
                    .map_err(writer_style_error)?;
                registry
            }
        };
        if styles.catalog().cell_formats.is_empty() {
            styles
                .register(CellStyle::default())
                .map_err(writer_style_error)?;
        }
        let date_styles = register_date_styles(&mut styles)?;
        let writer = Self {
            options,
            sheets: Vec::new(),
            styles: Some(styles),
            canonical_styles: false,
            date_styles,
            active: None,
            paused: Vec::new(),
            next_sheet: 0,
            row_buffer,
            temporary_bytes: 0,
            stats: WriteStats::default(),
            aborted: false,
            poisoned: false,
            cleanup_paths: Vec::new(),
            view_index: None,
        };
        if writer.style_bytes() > writer.options.max_metadata_bytes {
            return Err(limit("Writer metadata budget exceeded"));
        }
        Ok(writer)
    }
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
    /// Managed custom-theme storage; the default theme uses static storage.
    pub fn theme_memory_bytes(&self) -> usize {
        self.options.theme.memory_bytes()
    }
    fn catalog_bytes(&self) -> usize {
        self.options.theme.memory_bytes()
            + self.paused_bytes()
            + self.sheets.capacity() * size_of::<StoredSheet>()
            + self
                .sheets
                .iter()
                .map(|sheet| sheet.name.capacity() + sheet.file.path().as_os_str().len())
                .sum::<usize>()
            + self.active.as_ref().map_or(0, |sheet| {
                sheet.name.capacity()
                    + sheet.output.get_ref().path().as_os_str().len()
                    + sheet.footer.as_ref().map_or(0, Vec::capacity)
            })
    }
    fn style_bytes(&self) -> usize {
        self.style_memory_bytes()
            .saturating_add(self.options.theme.memory_bytes())
    }
    fn paused_bytes(&self) -> usize {
        self.paused.capacity() * size_of::<ActiveSheet>()
            + self
                .paused
                .iter()
                .map(|sheet| {
                    sheet.name.capacity()
                        + sheet.output.capacity()
                        + sheet.output.get_ref().path().as_os_str().len()
                        + sheet.footer.as_ref().map_or(0, Vec::capacity)
                })
                .sum::<usize>()
    }
    fn paused_footers(&self) -> u64 {
        self.paused
            .iter()
            .map(|sheet| sheet.footer.as_ref().map_or(FOOTER.len(), Vec::len) as u64)
            .sum()
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
        self.ensure_open()?;
        if views.is_none() && printing.is_none() {
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
        let maximum = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.style_memory_bytes())
            .saturating_sub(self.catalog_bytes());
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
            header.write_all(b"<sheetData>")?;
            if let Some(printing) = printing {
                footer.maximum = maximum.saturating_sub(header.data.capacity());
                footer.write_all(b"</sheetData>")?;
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
            .saturating_add(footer.data.capacity());
        self.start_sheet_with_header(
            name.into(),
            &header.data,
            printing.map(|_| footer.data),
            scratch_bytes,
        )
    }
    fn start_sheet_with_header(
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
            footer,
            visibility: crabxl_core::SheetVisibility::Visible,
        });
        self.write_active(header)?;
        self.next_sheet += 1;
        Ok(())
    }
    /// Write a complete sparse row. Validate/encode before spooling so invalid
    /// rows and budget failures do not partly commit worksheet content.
    pub fn write_row(&mut self, row: &Row) -> Result<()> {
        self.write_cells(row.index, row.cells.iter())
    }
    /// Write an explicitly materialized sparse sheet without cloning cell payloads.
    /// Style IDs must refer to this writer's registered formats. This creates a
    /// new sheet; it does not preserve parts of a loaded source package.
    pub fn write_worksheet(&mut self, sheet: &crabxl_core::Worksheet) -> Result<()> {
        self.start_sheet_with_settings(sheet.name(), sheet.sheet_views(), sheet.print_settings())?;
        let id = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?
            .id;
        self.set_sheet_visibility(id, sheet.visibility())?;
        let mut last = None;
        for index in sheet.row_indices() {
            self.write_cells(index, sheet.row_cells(index))?;
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
            return Err(state("A workbook requires at least one worksheet"));
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
        self.options.active_sheet = workbook
            .active_index()
            .ok_or_else(|| state("Workbook has no active sheet"))?;
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
    fn write_cells<'a>(
        &mut self,
        index: RowIndex,
        cells: impl Iterator<Item = &'a crabxl_core::Cell> + Clone,
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
        encode_cells(
            &mut self.row_buffer,
            index,
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
    /// Return progress and peak logical temporary storage.
    pub fn stats(&self) -> WriteStats {
        self.stats
    }
    /// Set the ZIP level for packaging without changing worksheet spools.
    pub fn set_compression_level(&mut self, level: Option<u8>) -> Result<()> {
        self.ensure_open()?;
        validate_compression_level(level)?;
        self.options.compression_level = level;
        Ok(())
    }
    /// Set the active display sheet before packaging. Finish validates the index.
    pub fn set_active_sheet(&mut self, index: usize) -> Result<()> {
        self.ensure_open()?;
        self.options.active_sheet = index;
        self.view_index = None;
        Ok(())
    }
    /// Request a relative, hidden or out-of-range view for compatible output.
    /// This does not change which temporary worksheet receives new rows.
    pub fn set_active_view_index(&mut self, index: i64) -> Result<()> {
        self.ensure_open()?;
        self.view_index = Some(index);
        Ok(())
    }
    /// Preflight a deferred view against stored, paused and active sheet states.
    /// No temporary files are packaged and no output sink is touched.
    pub fn active_view_selection(&self) -> Result<crabxl_core::ActiveViewSelection> {
        self.ensure_open()?;
        crabxl_core::normalize_active_view(
            self.view_index.unwrap_or(self.options.active_sheet as i64),
            self.next_sheet,
            |index| {
                self.sheets
                    .iter()
                    .find(|sheet| sheet.id == index)
                    .map(|sheet| sheet.visibility)
                    .or_else(|| {
                        self.paused
                            .iter()
                            .find(|sheet| sheet.id == index)
                            .map(|sheet| sheet.visibility)
                    })
                    .or_else(|| {
                        self.active
                            .as_ref()
                            .filter(|sheet| sheet.id == index)
                            .map(|sheet| sheet.visibility)
                    })
                    .unwrap_or(crabxl_core::SheetVisibility::Visible)
            },
        )
    }
    /// Set temporal storage before any rows are committed. Already serialized
    /// values cannot be reinterpreted by changing the epoch or ISO policy.
    pub fn set_temporal_options(&mut self, date_1904: bool, iso_dates: bool) -> Result<()> {
        self.ensure_open()?;
        if self.stats.rows != 0 {
            return Err(state(
                "Temporal options cannot change after rows are written",
            ));
        }
        self.options.date_1904 = date_1904;
        self.options.iso_dates = iso_dates;
        Ok(())
    }
    /// Bytes currently retained in owned temporary worksheets, including buffers.
    pub fn temporary_bytes(&self) -> u64 {
        self.temporary_bytes
    }
    /// Remove all owned temporary files without producing a workbook. Idempotent;
    /// attempts every cleanup even if one fails and retains failed paths for retry.
    /// Caller output has not been opened.
    pub fn abort(&mut self) -> Result<()> {
        self.aborted = true;
        let mut first_error = None;
        let mut remaining = Vec::new();
        for path in self.cleanup_paths.drain(..) {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != io::ErrorKind::NotFound
            {
                if first_error.is_none() {
                    first_error = Some(io_error(
                        "Cannot retry worksheet temporary-file cleanup",
                        error,
                    ));
                }
                remaining.push(path);
            }
        }
        for active in self.active.take().into_iter().chain(self.paused.drain(..)) {
            let (file, _) = active.output.into_parts();
            let path = file.path().to_owned();
            if let Err(error) = file.close()
                && error.kind() != io::ErrorKind::NotFound
            {
                if first_error.is_none() {
                    first_error = Some(io_error(
                        "Cannot remove active worksheet temporary file",
                        error,
                    ));
                }
                remaining.push(path);
            }
        }
        for sheet in self.sheets.drain(..) {
            let path = sheet.file.path().to_owned();
            if let Err(error) = sheet.file.close()
                && error.kind() != io::ErrorKind::NotFound
            {
                if first_error.is_none() {
                    first_error = Some(io_error("Cannot remove worksheet temporary file", error));
                }
                remaining.push(path);
            }
        }
        self.cleanup_paths = remaining;
        self.sheets = Vec::new();
        self.paused = Vec::new();
        self.styles = None;
        self.options.theme = crate::ThemeWritePolicy::Omit;
        self.row_buffer.data = Vec::new();
        self.temporary_bytes = 0;
        first_error.map_or(Ok(()), Err)
    }
    /// Package completed worksheets and return the output sink. An I/O failure
    /// may leave partial bytes in caller output; abort/Drop never imply save.
    pub fn finish<W: Write + Seek>(mut self, output: W) -> Result<W> {
        self.close_sheet()?;
        while let Some(active) = self.paused.pop() {
            self.active = Some(active);
            self.close_sheet()?;
        }
        self.sheets.sort_by_key(|sheet| sheet.id);
        if self.sheets.is_empty() {
            return Err(state("A workbook requires at least one worksheet"));
        }
        let active_view = if self.view_index.is_some() {
            Some(self.active_view_selection()?)
        } else {
            None
        };
        if active_view.is_none() && self.options.active_sheet >= self.sheets.len() {
            return Err(state("Active sheet index is outside the completed catalog"));
        }
        let first_visible = self
            .sheets
            .iter()
            .position(|sheet| sheet.visibility == crabxl_core::SheetVisibility::Visible)
            .ok_or_else(|| state("A workbook requires at least one visible sheet"))?;
        if active_view.is_none()
            && self.sheets[self.options.active_sheet].visibility
                != crabxl_core::SheetVisibility::Visible
        {
            self.options.active_sheet = self
                .sheets
                .iter()
                .enumerate()
                .skip(self.options.active_sheet)
                .find(|(_, sheet)| sheet.visibility == crabxl_core::SheetVisibility::Visible)
                .map_or(first_visible, |(index, _)| index);
        }
        let mut zip = ZipWriter::new(output);
        let options = compression_options(self.options.compression_level);
        for (index, sheet) in self.sheets.iter_mut().enumerate() {
            let part = format!("xl/worksheets/sheet{}.xml", index + 1);
            let size = sheet
                .file
                .as_file()
                .metadata()
                .map_err(|error| {
                    io_error("Cannot inspect worksheet temporary file", error).with_part(&part)
                })?
                .len();
            start_part(
                &mut zip,
                &part,
                options.large_file(size >= u64::from(u32::MAX)),
            )?;
            sheet.file.rewind().map_err(|error| {
                io_error("Cannot rewind worksheet temporary file", error).with_part(&part)
            })?;
            io::copy(
                &mut BufReader::with_capacity(self.options.buffer_bytes, sheet.file.as_file_mut()),
                &mut zip,
            )
            .map_err(|error| {
                io_error("Cannot package worksheet temporary file", error).with_part(part)
            })?;
        }
        package_metadata(
            &mut zip,
            &self.sheets,
            self.styles
                .as_ref()
                .ok_or_else(|| state("Writer style catalog is released"))?
                .catalog(),
            &self.options,
            options,
            active_view,
        )?;
        let mut output = zip
            .finish()
            .map_err(|error| zip_error("Cannot finalize XLSX ZIP", error))?;
        output
            .flush()
            .map_err(|error| io_error("Cannot flush XLSX output", error))?;
        self.abort()?;
        Ok(output)
    }
    fn ensure_open(&self) -> Result<()> {
        if self.aborted || self.poisoned {
            Err(state("Writer is aborted or failed"))
        } else {
            Ok(())
        }
    }
    fn check_temp(&self, additional: u64) -> Result<()> {
        if additional
            > self
                .options
                .max_temp_bytes
                .saturating_sub(self.temporary_bytes)
        {
            Err(limit("Writer temporary-storage limit exceeded"))
        } else {
            Ok(())
        }
    }
    fn write_active(&mut self, bytes: &[u8]) -> Result<()> {
        self.check_temp(bytes.len() as u64)?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        if bytes.len() as u64 > self.options.max_sheet_bytes.saturating_sub(active.bytes) {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        if let Err(error) = active.output.write_all(bytes) {
            self.poisoned = true;
            return Err(io_error("Cannot write worksheet temporary file", error));
        }
        active.bytes += bytes.len() as u64;
        self.temporary_bytes += bytes.len() as u64;
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        Ok(())
    }
}
fn register_date_styles(styles: &mut StyleRegistry) -> Result<DateStyleIds> {
    styles
        .register_temporal_presets_with_limit(usize::MAX)
        .map_err(writer_style_error)
}

impl Drop for WorkbookWriter {
    fn drop(&mut self) {
        let _ = self.abort();
    }
}

fn validate_sheet_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.chars().count() > 31
        || name.starts_with('\'')
        || name.ends_with('\'')
        || name.chars().any(|ch| ch < ' ' || ":\\/?*[]".contains(ch))
    {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid worksheet name"));
    }
    validate_xml_text(name)
}
pub(crate) fn validate_compression_level(level: Option<u8>) -> Result<()> {
    if level.is_some_and(|level| level > 9) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "ZIP compression level must be between 0 and 9",
        ));
    }
    Ok(())
}
pub(crate) fn compression_options(level: Option<u8>) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(if level == Some(0) {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        })
        .compression_level(level.filter(|level| *level != 0).map(i64::from))
}
fn state(message: &str) -> Error {
    Error::new(ErrorKind::InvalidState, message)
}
fn limit(message: &str) -> Error {
    Error::new(ErrorKind::LimitExceeded, message)
}
pub(crate) fn io_error(message: &str, error: io::Error) -> Error {
    Error::caused_by(ErrorKind::Io, message, error)
}
fn start_part<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    part: &str,
    options: SimpleFileOptions,
) -> Result<()> {
    zip.start_file(part, options)
        .map_err(|error| zip_error("Cannot start XLSX part", error).with_part(part))
}

fn write_part<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    options: SimpleFileOptions,
    write: impl FnOnce(&mut ZipWriter<W>) -> io::Result<()>,
) -> Result<()> {
    start_part(zip, name, options)?;
    write(zip).map_err(|error| io_error("Cannot write XLSX metadata part", error).with_part(name))
}
fn writer_style_error(error: Error) -> Error {
    if error.kind() == ErrorKind::MemoryBudgetExceeded {
        Error::caused_by(
            ErrorKind::LimitExceeded,
            "Writer style catalog allowance exceeded",
            error,
        )
    } else {
        error
    }
}
fn package_metadata<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    sheets: &[StoredSheet],
    styles: &StyleCatalog,
    configuration: &WriteOptions,
    options: SimpleFileOptions,
    active_view: Option<crabxl_core::ActiveViewSelection>,
) -> Result<()> {
    let date_1904 = configuration.date_1904;
    let active_sheet = configuration.active_sheet;
    let style_attributes = configuration.style_attributes;
    let theme = configuration.theme.bytes();
    write_part(zip, "[Content_Types].xml", options, |zip| {
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>")?;
        if theme.is_some() {
            zip.write_all(b"<Override PartName=\"/xl/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>")?;
        }
        for index in 1..=sheets.len() {
            write!(
                zip,
                "<Override PartName=\"/xl/worksheets/sheet{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
            )?;
        }
        zip.write_all(b"</Types>")
    })?;
    write_part(zip, "_rels/.rels", options, |zip| {
        write!(
            zip,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"
        )
    })?;
    write_part(zip, "xl/_rels/workbook.xml.rels", options, |zip| {
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">")?;
        for index in 1..=sheets.len() {
            write!(
                zip,
                "<Relationship Id=\"rId{index}\" Type=\"{REL}/worksheet\" Target=\"worksheets/sheet{index}.xml\"/>"
            )?;
        }
        write!(
            zip,
            "<Relationship Id=\"rId{}\" Type=\"{REL}/styles\" Target=\"styles.xml\"/>",
            sheets.len() + 1
        )?;
        if theme.is_some() {
            write!(
                zip,
                "<Relationship Id=\"rId{}\" Type=\"{REL}/theme\" Target=\"theme/theme1.xml\"/>",
                sheets.len() + 2
            )?;
        }
        zip.write_all(b"</Relationships>")
    })?;
    write_part(zip, "xl/workbook.xml", options, |zip| {
        write!(
            zip,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"{MAIN}\" xmlns:r=\"{REL}\"><workbookPr date1904=\"{}\"/>",
            u8::from(date_1904)
        )?;
        if let Some(view) = active_view {
            zip.write_all(b"<bookViews><workbookView")?;
            if let Some(index) = view.serialized_index {
                write!(zip, " activeTab=\"{index}\"")?;
            }
            zip.write_all(b"/></bookViews>")?;
        } else if active_sheet != 0 {
            write!(
                zip,
                "<bookViews><workbookView activeTab=\"{active_sheet}\"/></bookViews>"
            )?;
        }
        zip.write_all(b"<sheets>")?;
        for (index, sheet) in sheets.iter().enumerate() {
            let name = quick_xml::escape::escape(&sheet.name);
            write!(
                zip,
                "<sheet name=\"{name}\" sheetId=\"{}\" r:id=\"rId{}\"",
                index + 1,
                index + 1
            )?;
            if sheet.visibility != crabxl_core::SheetVisibility::Visible {
                write!(zip, " state=\"{}\"", sheet.visibility.as_str())?;
            }
            zip.write_all(b"/>")?;
        }
        zip.write_all(b"</sheets></workbook>")
    })?;
    if let Some(bytes) = theme {
        write_part(zip, "xl/theme/theme1.xml", options, |zip| {
            zip.write_all(bytes)
        })?;
    }
    write_part(zip, "xl/styles.xml", options, |zip| {
        crate::styles::write_styles(zip, styles, style_attributes)
    })
}

pub(crate) fn zip_error(message: &str, error: zip::result::ZipError) -> Error {
    let kind = if matches!(error, zip::result::ZipError::Io(_)) {
        ErrorKind::Io
    } else {
        ErrorKind::Archive
    };
    Error::caused_by(kind, message, error)
}
