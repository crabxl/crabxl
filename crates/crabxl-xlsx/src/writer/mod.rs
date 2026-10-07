// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

mod finish;
mod interleaved;
mod merged_rows;
mod rows;
mod sheet;
mod styles;
pub(crate) use merged_rows::{MergedRowCells, next_merged_row};

use crate::encode::{
    CellView, DateStyleIds, RowBuffer, StyleContext, ValueEncoding, validate_xml_text,
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
    dimensions: crabxl_core::SheetDimensions,
    header_prefix_bytes: u64,
    columns_written: bool,
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
    CanonicalCatalog(StyleCatalog),
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
    /// Adopt an owned-model catalog whose temporal IDs are already resolved.
    /// No automatic presets are appended, preserving exact borrowed-bank identity.
    pub fn from_canonical_style_catalog(
        options: WriteOptions,
        catalog: StyleCatalog,
    ) -> Result<Self> {
        Self::new_with_styles(options, StyleSource::CanonicalCatalog(catalog))
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
        let canonical_styles = matches!(
            &source,
            StyleSource::CanonicalCatalog(_) | StyleSource::Registry(_)
        );
        let mut styles = match source {
            StyleSource::Default => StyleRegistry::new(style_limits).map_err(writer_style_error)?,
            StyleSource::Catalog(catalog) | StyleSource::CanonicalCatalog(catalog) => {
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
        let date_styles = if canonical_styles {
            DateStyleIds {
                datetime: StyleId::new(0),
                time: StyleId::new(0),
                duration: StyleId::new(0),
                date: StyleId::new(0),
            }
        } else {
            register_date_styles(&mut styles)?
        };
        let writer = Self {
            options,
            sheets: Vec::new(),
            styles: Some(styles),
            canonical_styles,
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
