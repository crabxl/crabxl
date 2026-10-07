// SPDX-License-Identifier: MIT
// Workbook/relationship parsing adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

mod catalogs;
mod hyperlinks;
mod streams;
mod worksheet_metadata;

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
    visibility: crabxl_core::SheetVisibility,
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
    /// Workbook catalog visibility, including non-cell sheet kinds.
    pub const fn visibility(&self) -> crabxl_core::SheetVisibility {
        self.visibility
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
    active_sheet: i64,
    pub(crate) workbook_part: String,
    shared_string_part: Option<String>,
    shared_strings: Option<SharedStrings>,
    shared_string_options: SharedStringOptions,
    pub(crate) style_part: Option<String>,
    pub(crate) theme_part: Option<String>,
    theme: Option<crabxl_core::Theme>,
    imported_styles: Option<crate::style_reader::ImportedStyles>,
    styles_transferred: bool,
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
            return Err(limit(format!(
                "Compressed archive size {size} bytes exceeds max_archive_bytes={} bytes",
                limits.max_archive_bytes
            )));
        }
        source
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::caused_by(ErrorKind::Io, "Cannot seek archive", e))?;
        let mut archive = ZipArchive::new(source)
            .map_err(|e| Error::caused_by(ErrorKind::Archive, "Cannot read ZIP archive", e))?;
        if archive.len() > limits.max_archive_entries {
            return Err(limit("Archive entry count limit exceeded"));
        }
        if limits.max_total_uncompressed_bytes != u64::MAX {
            // ZIP's aggregate helper returns None for valid data-descriptor entries.
            // Read their central-directory sizes without decompressing any payload.
            let mut declared = 0u128;
            for index in 0..archive.len() {
                let entry = archive.by_index_raw(index).map_err(|error| {
                    Error::caused_by(ErrorKind::Archive, "Cannot inspect ZIP entry size", error)
                })?;
                declared += u128::from(entry.size());
            }
            if declared > u128::from(limits.max_total_uncompressed_bytes) {
                return Err(limit(format!(
                    "Declared uncompressed archive size {declared} bytes exceeds max_total_uncompressed_bytes={} bytes",
                    limits.max_total_uncompressed_bytes
                )));
            }
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
            styles_transferred: false,
            style_metadata_remaining: metadata_remaining,
        })
    }
    /// Inspect workbook sheets without loading their data.
    pub fn sheets(&self) -> &[SheetInfo] {
        &self.sheets
    }
    /// Original relationship-resolved SST identity, independent of conventional paths.
    pub(crate) fn source_strings_part(&self) -> Option<&str> {
        self.shared_string_part.as_deref()
    }
    /// First workbook view's active display position, or None if out of range.
    /// Missing view metadata defaults to the first sheet.
    pub fn active_index(&self) -> Option<usize> {
        crabxl_core::resolve_sheet_index(self.active_sheet, self.sheets.len())
    }
    /// Original signed workbook view index, before resolving a relative position.
    pub const fn active_view_index(&self) -> i64 {
        self.active_sheet
    }
    /// Whether the workbook uses the 1904 date origin.
    pub fn date_1904(&self) -> bool {
        self.date_1904
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
                let charge = 256_u64
                    .saturating_add(id.capacity() as u64)
                    .saturating_add(kind.capacity() as u64)
                    .saturating_add(target.capacity() as u64);
                *remaining = remaining.checked_sub(charge).ok_or_else(|| {
                    limit("Decoded relationship metadata exceeds allowance").with_part(part)
                })?;
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
) -> Result<(Vec<SheetInfo>, bool, i64)> {
    let mut xml = metadata_xml(archive, part, limits, remaining)?;
    let mut sheets = Vec::new();
    let mut names = HashSet::new();
    let mut date_1904 = false;
    let mut inside_sheets = false;
    let mut inside_views = false;
    let mut view_seen = false;
    let mut active_sheet = 0i64;
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
                        value.parse::<i64>().map_err(|error| {
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
                    visibility: match attribute(&e, b"state")?.as_deref() {
                        None | Some("visible") => crabxl_core::SheetVisibility::Visible,
                        Some("hidden") => crabxl_core::SheetVisibility::Hidden,
                        Some("veryHidden") => crabxl_core::SheetVisibility::VeryHidden,
                        _ => return Err(invalid("Invalid sheet visibility").with_part(part)),
                    },
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
