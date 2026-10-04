// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

use crate::encode::{DateEncoding, RowBuffer, encode_cells, validate_xml_text};
use crabxl_core::{
    CellStyle, DateEpoch, Error, ErrorKind, MAX_COLUMNS, Result, Row, RowIndex, StyleId,
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

/// Configurable resource limits for sequential worksheet spooling.
/// These bound managed buffers/metadata and temporary XML, not process RSS.
#[derive(Clone, Debug)]
pub struct WriteOptions {
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
    /// Zero-based active display sheet, checked against the completed catalog.
    pub active_sheet: usize,
}
impl Default for WriteOptions {
    fn default() -> Self {
        Self {
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
    name: String,
    file: NamedTempFile,
}
struct ActiveSheet {
    name: String,
    output: BufWriter<NamedTempFile>,
    bytes: u64,
    last_row: Option<RowIndex>,
}

/// One-shot XLSX writer using bounded rows and owned temporary worksheets.
///
/// Start sheets sequentially and write increasing row indices. Earlier sheets
/// and rows cannot be edited. Finish explicitly packages files into a seekable
/// output. Abort and Drop remove owned temporary files without publishing a
/// workbook. Caller-owned sinks can be passed to finish as &mut W.
pub struct WorkbookWriter {
    options: WriteOptions,
    sheets: Vec<StoredSheet>,
    styles: Vec<CellStyle>,
    active: Option<ActiveSheet>,
    row_buffer: RowBuffer,
    temporary_bytes: u64,
    stats: WriteStats,
    aborted: bool,
    poisoned: bool,
    cleanup_paths: Vec<PathBuf>,
}
impl WorkbookWriter {
    /// Create a writer with explicit resource and temporary-directory options.
    pub fn new(options: WriteOptions) -> Result<Self> {
        if options.max_styles < 5
            || options.max_styles > 65373
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
        let row_buffer = RowBuffer {
            data: Vec::new(),
            maximum: options.max_row_bytes,
        };
        let writer = Self {
            options,
            sheets: Vec::new(),
            styles: [
                "General",
                "yyyy-mm-dd hh:mm:ss.000",
                "hh:mm:ss.000",
                "[h]:mm:ss.000",
                "yyyy-mm-dd",
            ]
            .into_iter()
            .map(|format| CellStyle {
                number_format: format.into(),
                ..CellStyle::default()
            })
            .collect(),
            active: None,
            row_buffer,
            temporary_bytes: 0,
            stats: WriteStats::default(),
            aborted: false,
            poisoned: false,
            cleanup_paths: Vec::new(),
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
        if let Some(index) = self.styles.iter().position(|existing| *existing == style) {
            return Ok(StyleId::new(index as u32));
        }
        if self.styles.len() >= self.options.max_styles {
            return Err(limit("Writer shared style count limit exceeded"));
        }
        let bytes = (self.styles.len() + 1)
            .saturating_mul(size_of::<CellStyle>())
            .saturating_add(self.styles.iter().map(CellStyle::heap_bytes).sum::<usize>())
            .saturating_add(style.heap_bytes())
            .saturating_add(self.catalog_bytes());
        if bytes > self.options.max_metadata_bytes {
            return Err(limit("Writer metadata budget exceeded"));
        }
        self.styles.try_reserve_exact(1).map_err(|error| {
            Error::caused_by(
                ErrorKind::LimitExceeded,
                "Cannot allocate style table",
                error,
            )
        })?;
        let id = StyleId::new(self.styles.len() as u32);
        self.styles.push(style);
        Ok(id)
    }
    fn catalog_bytes(&self) -> usize {
        self.sheets.capacity() * size_of::<StoredSheet>()
            + self
                .sheets
                .iter()
                .map(|sheet| sheet.name.capacity() + sheet.file.path().as_os_str().len())
                .sum::<usize>()
            + self.active.as_ref().map_or(0, |sheet| {
                sheet.name.capacity() + sheet.output.get_ref().path().as_os_str().len()
            })
    }
    fn style_bytes(&self) -> usize {
        self.styles.capacity() * size_of::<CellStyle>()
            + self.styles.iter().map(CellStyle::heap_bytes).sum::<usize>()
    }
    /// Start a sheet, completing the previous one. Failed validation or temporary
    /// file creation leaves the previous active sheet usable.
    pub fn start_sheet(&mut self, name: impl Into<String>) -> Result<()> {
        self.ensure_open()?;
        let name = name.into();
        validate_sheet_name(&name)?;
        let count = self.sheets.len() + usize::from(self.active.is_some());
        if count >= self.options.max_sheets {
            return Err(limit("Writer sheet count limit exceeded"));
        }
        let folded = name.to_lowercase();
        if self
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
        let existing = self
            .sheets
            .iter()
            .map(|sheet| sheet.name.capacity() + sheet.file.path().as_os_str().len())
            .sum::<usize>()
            + self.active.as_ref().map_or(0, |sheet| {
                sheet.name.capacity() + sheet.output.get_ref().path().as_os_str().len()
            });
        if self
            .style_bytes()
            .saturating_add(existing)
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
            .saturating_add(name.capacity())
            .saturating_add(file.path().as_os_str().len())
            .saturating_add(self.sheets.capacity() * size_of::<StoredSheet>())
            > self.options.max_metadata_bytes
        {
            return Err(limit("Writer catalog budget exceeded"));
        }
        // Reserve both the old sheet's footer and new sheet before mutating state.
        let old_footer = if self.active.is_some() {
            FOOTER.len() as u64
        } else {
            0
        };
        self.check_temp(HEADER.len() as u64 + old_footer + FOOTER.len() as u64)?;
        if HEADER.len() as u64 + FOOTER.len() as u64 > self.options.max_sheet_bytes {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        self.close_sheet()?;
        self.active = Some(ActiveSheet {
            name,
            output: BufWriter::with_capacity(self.options.buffer_bytes, file),
            bytes: 0,
            last_row: None,
        });
        self.write_active(HEADER)?;
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
        self.start_sheet(sheet.name())?;
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
    /// Model epoch and active sheet are applied before any worksheet starts.
    pub fn write_workbook(&mut self, workbook: &crabxl_core::Workbook) -> Result<()> {
        self.ensure_open()?;
        if !self.sheets.is_empty() || self.active.is_some() {
            return Err(state("Workbook model export requires a fresh writer"));
        }
        if workbook.is_empty() {
            return Err(state("A workbook requires at least one worksheet"));
        }
        self.options.date_1904 = workbook.epoch() == DateEpoch::Mac1904;
        self.options.active_sheet = workbook
            .active_index()
            .ok_or_else(|| state("Workbook has no active sheet"))?;
        for (_, sheet) in workbook.sheets() {
            self.write_worksheet(sheet)?;
        }
        Ok(())
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
        encode_cells(
            &mut self.row_buffer,
            index,
            cells.clone(),
            self.options.max_cell_bytes,
            self.options.max_row_cells,
            &self.styles,
            DateEncoding {
                epoch: if self.options.date_1904 {
                    DateEpoch::Mac1904
                } else {
                    DateEpoch::Windows1900
                },
                iso_dates: self.options.iso_dates,
            },
        )
        .map_err(|error| error.with_part(&part))?;
        let length = self.row_buffer.data.len() as u64;
        self.check_temp(length + FOOTER.len() as u64)
            .map_err(|error| error.with_part(&part))?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        if active
            .bytes
            .saturating_add(length)
            .saturating_add(FOOTER.len() as u64)
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
        self.write_active(FOOTER)?;
        let active = self
            .active
            .take()
            .ok_or_else(|| state("No active worksheet"))?;
        let file = active.output.into_inner().map_err(|error| {
            self.poisoned = true;
            io_error("Cannot flush worksheet temporary file", error.into_error())
        })?;
        self.sheets.push(StoredSheet {
            name: active.name,
            file,
        });
        Ok(())
    }
    /// Return progress and peak logical temporary storage.
    pub fn stats(&self) -> WriteStats {
        self.stats
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
            if let Err(error) = std::fs::remove_file(&path) {
                if error.kind() != io::ErrorKind::NotFound {
                    if first_error.is_none() {
                        first_error = Some(io_error(
                            "Cannot retry worksheet temporary-file cleanup",
                            error,
                        ));
                    }
                    remaining.push(path);
                }
            }
        }
        if let Some(active) = self.active.take() {
            let (file, _) = active.output.into_parts();
            let path = file.path().to_owned();
            if let Err(error) = file.close() {
                if error.kind() != io::ErrorKind::NotFound {
                    if first_error.is_none() {
                        first_error = Some(io_error(
                            "Cannot remove active worksheet temporary file",
                            error,
                        ));
                    }
                    remaining.push(path);
                }
            }
        }
        for sheet in self.sheets.drain(..) {
            let path = sheet.file.path().to_owned();
            if let Err(error) = sheet.file.close() {
                if error.kind() != io::ErrorKind::NotFound {
                    if first_error.is_none() {
                        first_error =
                            Some(io_error("Cannot remove worksheet temporary file", error));
                    }
                    remaining.push(path);
                }
            }
        }
        self.cleanup_paths = remaining;
        self.sheets = Vec::new();
        self.styles = Vec::new();
        self.row_buffer.data = Vec::new();
        self.temporary_bytes = 0;
        first_error.map_or(Ok(()), Err)
    }
    /// Package completed worksheets and return the output sink. An I/O failure
    /// may leave partial bytes in caller output; abort/Drop never imply save.
    pub fn finish<W: Write + Seek>(mut self, output: W) -> Result<W> {
        self.close_sheet()?;
        if self.sheets.is_empty() {
            return Err(state("A workbook requires at least one worksheet"));
        }
        if self.options.active_sheet >= self.sheets.len() {
            return Err(state("Active sheet index is outside the completed catalog"));
        }
        let mut zip = ZipWriter::new(output);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
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
            &self.styles,
            self.options.date_1904,
            self.options.active_sheet,
            options,
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
fn package_metadata<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    sheets: &[StoredSheet],
    styles: &[CellStyle],
    date_1904: bool,
    active_sheet: usize,
    options: SimpleFileOptions,
) -> Result<()> {
    write_part(zip, "[Content_Types].xml", options, |zip| {
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>")?;
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
            "<Relationship Id=\"rId{}\" Type=\"{REL}/styles\" Target=\"styles.xml\"/></Relationships>",
            sheets.len() + 1
        )
    })?;
    write_part(zip, "xl/workbook.xml", options, |zip| {
        write!(
            zip,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"{MAIN}\" xmlns:r=\"{REL}\"><workbookPr date1904=\"{}\"/>",
            u8::from(date_1904)
        )?;
        if active_sheet != 0 {
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
                "<sheet name=\"{name}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
                index + 1,
                index + 1
            )?;
        }
        zip.write_all(b"</sheets></workbook>")
    })?;
    write_part(zip, "xl/styles.xml", options, |zip| {
        crate::styles::write_styles(zip, styles)
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
