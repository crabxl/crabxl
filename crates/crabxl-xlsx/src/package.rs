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
        })
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
    pub(crate) fn collect_sheet(&mut self, name: &str, maximum: usize) -> Result<SheetData> {
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
        let mut stream = self.rows(name)?;
        let mut sheet = SheetData { rows: Vec::new() };
        let mut cell_bytes = 0usize;
        while let Some(row) = stream.next_row()? {
            let row_bytes = row.memory_bytes().saturating_sub(size_of::<Row>());
            cell_bytes = cell_bytes
                .checked_add(row_bytes)
                .ok_or_else(materialization_limit)?;
            let available_rows = maximum
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
            if retained > maximum {
                return Err(materialization_limit());
            }
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
        if self.shared_strings.is_none() {
            if let Some(string_part) = &self.shared_string_part {
                let file = self.archive.by_name(string_part).map_err(|e| {
                    Error::caused_by(ErrorKind::Archive, "Cannot open shared-string part", e)
                        .with_part(string_part)
                })?;
                if file.size() > self.limits.max_part_bytes {
                    return Err(
                        limit("Shared-string part size limit exceeded").with_part(string_part)
                    );
                }
                let strings = SharedStrings::parse(
                    BufReader::with_capacity(self.limits.input_buffer_bytes, file),
                    string_part.clone(),
                    self.limits,
                    &self.shared_string_options,
                )?;
                self.shared_strings = Some(strings);
            }
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
        )
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
                    && e.local_name().as_ref() == b"Relationships";
                if !valid_root {
                    return Err(invalid("Invalid relationships root").with_part(part));
                }
            }
            Event::Start(e)
                if frame.scope == Scope::Relationships
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"Relationship" =>
            {
                if relationships.len() >= limits.max_archive_entries {
                    return Err(limit("Relationship count limit exceeded").with_part(part));
                }
                let id = required_attribute(&e, b"Id", frame.decoder)?;
                let kind = required_attribute(&e, b"Type", frame.decoder)?;
                let target = required_attribute(&e, b"Target", frame.decoder)?;
                let mode = attribute(&e, b"TargetMode", frame.decoder)?;
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
                        || e.local_name().as_ref() != b"Types") =>
            {
                return Err(invalid("Invalid content types root").with_part(part));
            }
            Event::Start(e)
                if frame.scope == Scope::ContentTypes
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"Override" =>
            {
                if types.len() >= limits.max_archive_entries {
                    return Err(limit("Content type count limit exceeded").with_part(part));
                }
                let name = required_attribute(&e, b"PartName", frame.decoder)?;
                let content_type = required_attribute(&e, b"ContentType", frame.decoder)?;
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
                        || e.local_name().as_ref() != b"workbook") =>
            {
                return Err(invalid("Invalid workbook root").with_part(part));
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"sheets" =>
            {
                inside_sheets = true;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref() == b"sheets" =>
            {
                inside_sheets = false;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"bookViews" =>
            {
                inside_views = true;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref() == b"bookViews" =>
            {
                inside_views = false;
            }
            Event::Start(e)
                if inside_views
                    && !view_seen
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref() == b"workbookView" =>
            {
                active_sheet = attribute(&e, b"activeTab", frame.decoder)?
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
                    && e.local_name().as_ref() == b"workbookPr" =>
            {
                date_1904 = match attribute(&e, b"date1904", frame.decoder)?.as_deref() {
                    None | Some("0" | "false") => false,
                    Some("1" | "true") => true,
                    _ => return Err(invalid("Invalid date1904 flag").with_part(part)),
                };
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && inside_sheets
                    && frame.depth == 3
                    && e.local_name().as_ref() == b"sheet" =>
            {
                if sheets.len() >= limits.max_sheets {
                    return Err(limit("Worksheet catalog limit exceeded").with_part(part));
                }
                let name = required_attribute(&e, b"name", frame.decoder)?;
                if name.is_empty() || !names.insert(name.clone()) {
                    return Err(invalid("Empty or duplicate sheet name").with_part(part));
                }
                let id = frame
                    .sheet_relationship
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
