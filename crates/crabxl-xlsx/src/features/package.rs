//! Feature parts in the package: reading tables, comments and document
//! properties for a `WorkbookReader`, and building new parts for the writer
//! and the editor.
use super::sheet::PartRefs;
use super::*;
use crate::WorkbookReader;
use crate::xml::{Scope, XmlStream};
use crabxl_core::{Result, SheetFeatures, WorkbookFeatures};
use quick_xml::events::Event;
use std::io::{self, BufReader, Read, Seek, Write};

/// Feature parts that a source worksheet references.
#[derive(Clone, Debug, Default)]
pub(crate) struct SourceParts {
    /// Relationship id and part name of each table.
    pub tables: Vec<(String, String)>,
    /// Relationship id and part name of the comments part.
    pub comments: Option<(String, String)>,
    /// Relationship id and part name of the legacy VML drawing.
    pub vml: Option<(String, String)>,
    /// Spreadsheet drawing relationship and part.
    pub drawing: Option<(String, String)>,
    pub pivots: Vec<(String, String)>,
    pub printer: Option<(String, String)>,
}
fn kind_is(kind: &str, suffix: &str) -> bool {
    kind.rsplit('/').next() == Some(suffix)
}

impl<R: Read + Seek> WorkbookReader<R> {
    /// Retain small assets in memory and spool larger assets without a full-file buffer.
    pub(crate) fn part_payload(&mut self, part: &str) -> Result<crabxl_core::BinaryPayload> {
        let maximum = self.limits.max_part_bytes;
        let mut input = self.archive.by_name(part).map_err(|error| {
            crabxl_core::Error::caused_by(
                crabxl_core::ErrorKind::Archive,
                "Cannot open encoded asset",
                error,
            )
            .with_part(part)
        })?;
        if input.size() > maximum {
            return Err(crabxl_core::Error::new(
                crabxl_core::ErrorKind::LimitExceeded,
                "Encoded asset exceeds part byte limit",
            )
            .with_part(part));
        }
        let threshold = self
            .limits
            .input_buffer_bytes
            .min(self.limits.max_materialized_bytes);
        if input.size() <= threshold as u64 {
            let mut data = Vec::new();
            input
                .by_ref()
                .take(maximum.saturating_add(1))
                .read_to_end(&mut data)
                .map_err(|error| {
                    crabxl_core::Error::caused_by(
                        crabxl_core::ErrorKind::Io,
                        "Cannot read encoded asset",
                        error,
                    )
                    .with_part(part)
                })?;
            if data.len() as u64 > maximum {
                return Err(crabxl_core::Error::new(
                    crabxl_core::ErrorKind::LimitExceeded,
                    "Encoded asset exceeds part byte limit",
                )
                .with_part(part));
            }
            return Ok(data.into());
        }
        let mut output = tempfile::tempfile().map_err(|error| {
            crabxl_core::Error::caused_by(
                crabxl_core::ErrorKind::Io,
                "Cannot create encoded asset spool",
                error,
            )
        })?;
        let length =
            io::copy(&mut input.take(maximum.saturating_add(1)), &mut output).map_err(|error| {
                crabxl_core::Error::caused_by(
                    crabxl_core::ErrorKind::Io,
                    "Cannot spool encoded asset",
                    error,
                )
                .with_part(part)
            })?;
        if length > maximum {
            return Err(crabxl_core::Error::new(
                crabxl_core::ErrorKind::LimitExceeded,
                "Encoded asset exceeds part byte limit",
            )
            .with_part(part));
        }
        crabxl_core::BinaryPayload::from_file(output).map_err(|error| {
            crabxl_core::Error::caused_by(
                crabxl_core::ErrorKind::Io,
                "Cannot retain encoded asset spool",
                error,
            )
            .with_part(part)
        })
    }
    pub(crate) fn sheet_part(&self, name: &str) -> Result<String> {
        let info = self
            .sheets()
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| {
                crabxl_core::Error::new(
                    crabxl_core::ErrorKind::SheetNotFound,
                    "Worksheet not found",
                )
            })?;
        // Dialog sheets are read like worksheets, as in openpyxl.
        Ok(info.part().to_owned())
    }

    pub(crate) fn part_stream(
        &mut self,
        part: &str,
    ) -> Result<XmlStream<BufReader<zip::read::ZipFile<'_, R>>>> {
        let byte_limit = if self.sheets().iter().any(|sheet| sheet.part() == part) {
            self.limits.max_part_bytes
        } else {
            self.limits
                .max_part_bytes
                .min(self.limits.max_metadata_bytes)
        };
        let file = self.archive.by_name(part).map_err(|error| {
            crabxl_core::Error::caused_by(
                crabxl_core::ErrorKind::Archive,
                "Cannot open feature part",
                error,
            )
            .with_part(part)
        })?;
        Ok(XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part.to_owned(),
            byte_limit,
            self.limits,
        ))
    }

    pub(crate) fn part_bytes(&mut self, part: &str) -> Result<Vec<u8>> {
        let maximum = self.limits.max_part_bytes;
        let file = self.archive.by_name(part).map_err(|error| {
            crabxl_core::Error::caused_by(
                crabxl_core::ErrorKind::Archive,
                "Cannot open feature part",
                error,
            )
            .with_part(part)
        })?;
        let mut bytes = Vec::new();
        file.take(maximum.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| {
                crabxl_core::Error::caused_by(
                    crabxl_core::ErrorKind::Io,
                    "Cannot read feature part",
                    error,
                )
                .with_part(part)
            })?;
        if bytes.len() as u64 > maximum {
            return Err(crabxl_core::Error::new(
                crabxl_core::ErrorKind::LimitExceeded,
                "Feature part exceeds part byte limit",
            )
            .with_part(part));
        }
        Ok(bytes)
    }

    /// Read tables, filters, conditional formats, validation, comments,
    /// protection and header/footer of one worksheet. The worksheet XML is
    /// scanned once without decoding cells.
    pub fn sheet_features(&mut self, name: &str) -> Result<SheetFeatures> {
        self.sheet_features_with_sources(name)
            .map(|(features, _)| features)
    }

    pub(crate) fn sheet_features_with_sources(
        &mut self,
        name: &str,
    ) -> Result<(SheetFeatures, SourceParts)> {
        let part = self.sheet_part(name)?;
        let chartsheet = self
            .sheets()
            .iter()
            .find(|sheet| sheet.name() == name)
            .is_some_and(|sheet| sheet.kind() == crate::SheetKind::ChartSheet);
        let (mut features, refs) = if chartsheet {
            let mut xml = self.part_stream(&part)?;
            let root = crate::drawing::read_chartsheet(&mut xml)?;
            let mut refs = PartRefs::default();
            for element in root.children.iter().filter_map(|child| match child {
                crabxl_core::XmlContent::Element(element) => Some(element),
                _ => None,
            }) {
                if element.local_name() == "drawing" {
                    refs.drawing = element.attributes.iter().find_map(|(name, value)| {
                        let (prefix, local) = name.split_once(':')?;
                        if local != "id" {
                            return None;
                        }
                        let namespace = format!("xmlns:{prefix}");
                        let uri = element
                            .attribute(&namespace)
                            .or_else(|| root.attribute(&namespace))?;
                        matches!(
                            uri,
                            crate::xml::OFFICE_REL_URI | crate::xml::STRICT_OFFICE_REL_URI
                        )
                        .then(|| value.to_string())
                    });
                    if refs.drawing.is_none() {
                        return Err(invalid("Chartsheet drawing has no relationship"));
                    }
                }
            }
            (
                SheetFeatures {
                    chartsheet: Some(root),
                    ..Default::default()
                },
                refs,
            )
        } else {
            let mut xml = self.part_stream(&part)?;
            super::sheet::read_worksheet(&mut xml).map_err(|error| error.with_part(&part))?
        };
        let mut sources = SourceParts::default();
        let rels_part = crate::package::relationship_part(&part);
        if self.archive.index_for_name(&rels_part).is_none() {
            if !refs.is_empty() {
                return Err(invalid("Worksheet feature relationships are missing").with_part(part));
            }
            return Ok((features, sources));
        }
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &rels_part,
            self.limits,
            &mut remaining,
        )?;
        let target = |id: &str| -> Result<String> {
            let relationship = relationships
                .get(id)
                .ok_or_else(|| invalid("Unknown feature relationship id"))?;
            if relationship.external {
                return Err(invalid("Feature part is outside the package"));
            }
            crate::package::resolve_part(&part, &relationship.target)
        };
        if let Some(id) = &refs.printer {
            let rel = relationships
                .get(id)
                .ok_or_else(|| invalid("Missing printer relationship"))?;
            if rel.external || !kind_is(&rel.kind, "printerSettings") {
                return Err(invalid("Invalid printer relationship"));
            }
            sources.printer = Some((id.clone(), target(id)?));
        }
        let pivot_ids = if refs.pivots.is_empty() {
            crate::pivot::implicit_pivot_ids(&relationships)
        } else {
            refs.pivots
        };
        for id in &pivot_ids {
            sources.pivots.push((id.clone(), target(id)?));
        }
        for id in &refs.tables {
            sources.tables.push((id.clone(), target(id)?));
        }
        if let Some(id) = &refs.drawing {
            let rel = relationships
                .get(id)
                .ok_or_else(|| invalid("Missing drawing relationship"))?;
            if !kind_is(&rel.kind, "drawing") {
                return Err(invalid("Wrong drawing relationship type"));
            }
            sources.drawing = Some((id.clone(), target(id)?));
        }
        if let Some(id) = &refs.legacy_drawing {
            sources.vml = Some((id.clone(), target(id)?));
        }
        let mut comment_ids: Vec<&String> = relationships
            .iter()
            .filter(|(_, rel)| !rel.external && kind_is(&rel.kind, "comments"))
            .map(|(id, _)| id)
            .collect();
        comment_ids.sort();
        if let Some(id) = comment_ids.first() {
            sources.comments = Some(((*id).clone(), target(id)?));
        }
        for (_, table_part) in &sources.tables {
            let mut xml = self.part_stream(table_part)?;
            features.tables.push(
                super::table::read_part(&mut xml).map_err(|error| error.with_part(table_part))?,
            );
        }
        if let Some((_, comments_part)) = &sources.comments {
            {
                let mut xml = self.part_stream(comments_part)?;
                features.comments = super::comment::read_part(&mut xml)
                    .map_err(|error| error.with_part(comments_part))?;
            }
            if let Some((_, vml_part)) = &sources.vml {
                let bytes = self.part_bytes(vml_part)?;
                super::comment::apply_vml(&bytes, &mut features.comments)
                    .map_err(|error| error.with_part(vml_part))?;
            }
        }
        if let Some((id, part)) = &sources.printer {
            let data = self.part_payload(part)?;
            let (relationships, parts) = self.read_part_dependencies(part)?;
            features.printer_settings = Some(crabxl_core::features::PrinterSettings {
                relationship: id.clone().into(),
                data,
                source_part: Some(part.clone().into()),
                relationships,
                parts,
            });
        }
        if chartsheet {
            let root = features
                .chartsheet
                .as_ref()
                .ok_or_else(|| invalid("Missing chartsheet properties"))?;
            features.chartsheet_parts = Some(self.read_object_parts(
                &part,
                root,
                refs.drawing.as_deref().unwrap_or(""),
                true,
            )?);
        }
        Ok((features, sources))
    }

    /// Read defined names, workbook protection and document properties.
    pub fn workbook_features(&mut self) -> Result<WorkbookFeatures> {
        let mut features = WorkbookFeatures::default();
        let mut parent = None;
        let calculation_limit =
            usize::try_from(self.limits.max_metadata_bytes).unwrap_or(usize::MAX);
        let workbook_part = self.workbook_part.clone();
        {
            let mut xml = self.part_stream(&workbook_part)?;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e) if frame.depth == 1 => {
                        if e.len() > calculation_limit {
                            return Err(crabxl_core::Error::new(
                                crabxl_core::ErrorKind::LimitExceeded,
                                "Workbook namespace context exceeds metadata allowance",
                            ));
                        }
                        let mut context = crate::drawing_xml::element(&e)?;
                        context.attributes.retain(|(name, _)| {
                            name.as_ref() == "xmlns" || name.starts_with("xmlns:")
                        });
                        parent = Some(context);
                    }

                    Event::Start(e) if frame.depth == 2 && frame.scope == Scope::Spreadsheet => {
                        let e = e.into_owned();
                        match e.local_name().as_ref().as_bytes() {
                            b"definedNames" => {
                                features.defined_names =
                                    super::defined_name::read(&mut xml, &e, 2)?;
                            }
                            b"calcPr" | b"bookViews" | b"workbookPr" => {
                                if e.len() > calculation_limit {
                                    return Err(crabxl_core::Error::new(
                                        crabxl_core::ErrorKind::LimitExceeded,
                                        "Calculation properties exceed metadata allowance",
                                    ));
                                }
                                let mut remaining = calculation_limit;
                                let mut calculation = crate::drawing_xml::read_limited(
                                    &mut xml,
                                    &e,
                                    2,
                                    &mut remaining,
                                )?;
                                if let Some(parent) = &parent {
                                    crate::drawing_xml::inherit(&mut calculation, parent);
                                }
                                if std::mem::size_of::<crabxl_core::XmlElement>()
                                    + calculation.heap_bytes()
                                    > calculation_limit
                                {
                                    return Err(crabxl_core::Error::new(
                                        crabxl_core::ErrorKind::LimitExceeded,
                                        "Calculation properties exceed metadata allowance",
                                    ));
                                }
                                if e.local_name().as_ref() == "workbookPr" {
                                    features.workbook_properties = Some(calculation);
                                } else if e.local_name().as_ref() == "bookViews" {
                                    features.views = Some(calculation);
                                } else {
                                    features.calculation = Some(calculation);
                                }
                            }
                            b"workbookProtection" => {
                                features.protection =
                                    Some(super::protection::read_workbook(&mut xml, &e, 2)?);
                            }
                            _ => crate::style_codec::skip(&mut xml, 2)?,
                        }
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
        if self.archive.index_for_name("_rels/.rels").is_some() {
            let mut remaining = self.limits.max_metadata_bytes;
            let relationships = crate::package::read_relationships(
                &mut self.archive,
                "_rels/.rels",
                self.limits,
                &mut remaining,
            )?;
            let mut parts: Vec<(u8, String)> = Vec::new();
            for rel in relationships.values() {
                if rel.external {
                    continue;
                }
                let order = if kind_is(&rel.kind, "core-properties") {
                    0
                } else if kind_is(&rel.kind, "extended-properties") {
                    1
                } else if kind_is(&rel.kind, "custom-properties") {
                    2
                } else {
                    continue;
                };
                let part = crate::package::resolve_part("", &rel.target)?;
                if self.archive.index_for_name(&part).is_some() {
                    parts.push((order, part));
                }
            }
            parts.sort();
            for (order, part) in parts {
                let mut xml = self.part_stream(&part)?;
                let props = &mut features.properties;
                match order {
                    0 => super::properties::read_core(&mut xml, props),
                    1 => super::properties::read_app(&mut xml, props),
                    _ => super::properties::read_custom(&mut xml, props),
                }
                .map_err(|error| error.with_part(&part))?;
            }
        }
        Ok(features)
    }
}

/// A new package part.
#[derive(Clone, Debug)]
pub(crate) struct OutputPart {
    pub name: String,
    pub content_type: Box<str>,
    pub bytes: crabxl_core::BinaryPayload,
}

/// A new worksheet relationship.
#[derive(Clone, Debug)]
pub(crate) struct OutputRelationship {
    pub id: String,
    pub kind: Box<str>,
    pub target: String,
}
impl OutputRelationship {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        if self.id.is_empty() || self.kind.is_empty() || self.target.is_empty() {
            return Err(io::Error::other("Empty package relationship field"));
        }
        for value in [self.id.as_str(), self.kind.as_ref(), self.target.as_str()] {
            crate::encode::validate_xml_text(value).map_err(io::Error::other)?;
        }
        out.write_all(b"<Relationship Id=\"")?;
        out.write_all(quick_xml::escape::escape(self.id.as_str()).as_bytes())?;
        write!(
            out,
            "\" Type=\"{}\" Target=\"",
            quick_xml::escape::escape(self.kind.as_ref())
        )?;
        out.write_all(quick_xml::escape::escape(self.target.as_str()).as_bytes())?;
        out.write_all(b"\"/>")
    }
}

pub(crate) fn validate_relationship_ids(bytes: &[u8]) -> Result<()> {
    let mut xml = XmlStream::new(
        io::Cursor::new(bytes),
        "relationships".into(),
        bytes.len() as u64,
        crabxl_core::ResourceLimits::default(),
    );
    let mut ids = std::collections::HashSet::new();
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == 2
                    && frame.scope == Scope::Relationships
                    && e.local_name().as_ref() == "Relationship" =>
            {
                let id = crate::xml::required_attribute(&e, b"Id")?;
                if id.is_empty() || !ids.insert(id) {
                    return Err(invalid("Empty or duplicate package relationship id"));
                }
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PivotCacheOutput {
    pub id: u32,
    pub part: String,
    pub relationship: String,
}
/// Feature parts and relationships for one worksheet.
#[derive(Clone, Debug, Default)]
pub(crate) struct SheetOutput {
    pub refs: PartRefs,
    pub parts: Vec<OutputPart>,
    pub relationships: Vec<OutputRelationship>,
    pub pivot_caches: Vec<PivotCacheOutput>,
    pub extra_relationships: Vec<u8>,
}
impl SheetOutput {
    pub(crate) fn heap_bytes(&self) -> usize {
        self.extra_relationships.capacity()
            + self
                .parts
                .iter()
                .map(|part| part.name.capacity() + part.bytes.memory_bytes() + 64)
                .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| rel.id.capacity() + rel.target.capacity() + 64)
                .sum::<usize>()
            + self
                .pivot_caches
                .iter()
                .map(|cache| {
                    cache.part.capacity() * 4
                        + cache.relationship.capacity()
                        + size_of::<PivotCacheOutput>()
                        + 4 * size_of::<(usize, PivotCacheAllocation)>()
                        + size_of::<crabxl_core::features::PivotCache>()
                        + 2 * size_of::<usize>()
                })
                .sum::<usize>()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PivotCacheAllocation {
    pub cache: std::sync::Weak<crabxl_core::features::PivotCache>,
    pub id: u32,
    pub part: String,
}

/// Part-number allocator that keeps new part names and table ids unique.
#[derive(Clone, Debug)]
pub(crate) struct PartNumbers {
    pub table_file: u32,
    pub table_id: u64,
    pub comments_file: u32,
    pub vml_file: u32,
    pub drawing_file: u32,
    pub image_file: u32,
    pub chart_file: u32,
    pub related_file: u32,
    pub pivot_file: u32,
    pub pivot_cache_file: u32,
    pub pivot_cache_id: u64,
    pub pivot_caches: std::collections::HashMap<usize, PivotCacheAllocation>,
    pub external_file: u32,
    pub vba_file: u32,
    pub metadata_file: u32,
    taken: std::collections::HashSet<String>,
}
impl Default for PartNumbers {
    fn default() -> Self {
        Self {
            table_file: 1,
            table_id: 1,
            comments_file: 1,
            vml_file: 1,
            drawing_file: 1,
            image_file: 1,
            chart_file: 1,
            related_file: 1,
            pivot_file: 1,
            pivot_cache_file: 1,
            pivot_cache_id: 0,
            pivot_caches: Default::default(),
            external_file: 1,
            vba_file: 1,
            metadata_file: 1,
            taken: Default::default(),
        }
    }
}
impl PartNumbers {
    /// Allocator that avoids `existing` part names.
    pub(crate) fn avoiding<'a>(existing: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            taken: existing.into_iter().map(str::to_owned).collect(),
            ..Self::default()
        }
    }
    /// Reserve table ids up to and including `id`.
    pub(crate) fn reserve_table_id(&mut self, id: u32) {
        self.table_id = self.table_id.max(u64::from(id) + 1);
    }
    pub(crate) fn next_name(
        &mut self,
        counter: fn(&mut Self) -> &mut u32,
        pattern: (&str, &str),
    ) -> String {
        loop {
            let number = *counter(self);
            *counter(self) += 1;
            let name = format!("{}{number}{}", pattern.0, pattern.1);
            if self.taken.insert(name.clone()) {
                return name;
            }
        }
    }
}

/// Relative target from a worksheet part to `part` (both package names).
pub(crate) fn relative_target(sheet_part: &str, part: &str) -> String {
    let from: Vec<&str> = sheet_part.split('/').collect();
    let to: Vec<&str> = part.split('/').collect();
    let folder = &from[..from.len().saturating_sub(1)];
    let common = folder
        .iter()
        .zip(to.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut target = String::new();
    for _ in common..folder.len() {
        target.push_str("../");
    }
    target.push_str(&to[common..].join("/"));
    target
}

/// Build feature parts for one worksheet. `relationship_id` makes ids unique
/// within the worksheet relationships part.
pub(crate) fn build_sheet_output(
    features: &SheetFeatures,
    sheet_index: usize,
    sheet_part: &str,
    numbers: &mut PartNumbers,
    mut relationship_id: impl FnMut(&str) -> String,
) -> io::Result<SheetOutput> {
    let mut output = SheetOutput::default();
    if let Some(graph) = &features.chartsheet_parts {
        crate::drawing::build_part_dependencies(
            &graph.relationships,
            &graph.parts,
            graph.source_part.as_deref(),
            sheet_part,
            numbers,
            &mut output,
        )?;
        output.extra_relationships =
            crate::drawing::take_relationships(&mut output.parts, sheet_part)?;
    }
    let reserved: std::collections::HashSet<_> = features
        .chartsheet_parts
        .iter()
        .flat_map(|graph| graph.relationships.iter().map(|rel| rel.id.as_ref()))
        .chain(
            features
                .printer_settings
                .iter()
                .map(|settings| settings.relationship.as_ref()),
        )
        .collect();
    let mut allocate = |kind: &str| {
        let mut id = relationship_id(kind);
        while reserved.contains(id.as_str()) {
            id.push('_');
        }
        id
    };
    if let Some(settings) = &features.printer_settings {
        if settings.relationship.is_empty() {
            return Err(io::Error::other("Empty printer relationship"));
        }
        let part = numbers.next_name(
            |n| &mut n.related_file,
            ("xl/printerSettings/printerSettings", ".bin"),
        );
        output.relationships.push(OutputRelationship { id: settings.relationship.to_string(), kind: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/printerSettings".into(), target: relative_target(sheet_part, &part) });
        output.parts.push(OutputPart { name: part.clone(), content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings".into(), bytes: settings.data.clone() });
        crate::drawing::build_part_dependencies(
            &settings.relationships,
            &settings.parts,
            settings.source_part.as_deref(),
            &part,
            numbers,
            &mut output,
        )?;
    }
    for table in &features.tables {
        let name = numbers.next_name(|n| &mut n.table_file, ("xl/tables/table", ".xml"));
        let id = u32::try_from(numbers.table_id)
            .map_err(|_| io::Error::other("Table identifier space exhausted"))?;
        numbers.table_id += 1;
        let mut bytes = Vec::new();
        super::table::write_part(&mut bytes, table, id)?;
        let rid = allocate("table");
        output.relationships.push(OutputRelationship {
            id: rid.clone(),
            kind: REL_TABLE.into(),
            target: relative_target(sheet_part, &name),
        });
        output.refs.tables.push(rid);
        output.parts.push(OutputPart {
            name,
            content_type: CT_TABLE.into(),
            bytes: bytes.into(),
        });
    }
    if !features.comments.is_empty() {
        let name = numbers.next_name(|n| &mut n.comments_file, ("xl/comments/comment", ".xml"));
        let mut bytes = Vec::new();
        super::comment::write_part(&mut bytes, &features.comments)?;
        output.relationships.push(OutputRelationship {
            id: allocate("comments"),
            kind: REL_COMMENTS.into(),
            target: relative_target(sheet_part, &name),
        });
        output.parts.push(OutputPart {
            name,
            content_type: CT_COMMENTS.into(),
            bytes: bytes.into(),
        });
        let name = numbers.next_name(|n| &mut n.vml_file, ("xl/drawings/commentsDrawing", ".vml"));
        let mut bytes = Vec::new();
        super::comment::write_vml(&mut bytes, &features.comments, sheet_index)?;
        let rid = allocate("vml");
        output.relationships.push(OutputRelationship {
            id: rid.clone(),
            kind: REL_VML.into(),
            target: relative_target(sheet_part, &name),
        });
        output.refs.legacy_drawing = Some(rid);
        output.parts.push(OutputPart {
            name,
            content_type: CT_VML.into(),
            bytes: bytes.into(),
        });
    }
    crate::drawing::build_output(
        features.images.as_deref().unwrap_or(&[]),
        features.charts.as_deref().unwrap_or(&[]),
        features.shapes.as_deref().unwrap_or(&[]),
        sheet_part,
        numbers,
        &mut allocate,
        &mut output,
    )?;
    if let Some(pivots) = &features.pivots {
        crate::pivot::build_output(pivots, sheet_part, numbers, &mut allocate, &mut output)?;
    }
    Ok(output)
}

pub(crate) const REL_CORE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
pub(crate) const REL_APP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";
pub(crate) const REL_CUSTOM: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
pub(crate) const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
pub(crate) const CT_APP: &str =
    "application/vnd.openxmlformats-officedocument.extended-properties+xml";
pub(crate) const CT_CUSTOM: &str =
    "application/vnd.openxmlformats-officedocument.custom-properties+xml";

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn relative_targets() {
        assert_eq!(
            relative_target("xl/worksheets/sheet1.xml", "xl/tables/table1.xml"),
            "../tables/table1.xml"
        );
        assert_eq!(
            relative_target("xl/worksheets/sheet1.xml", "xl/worksheets/x.xml"),
            "x.xml"
        );
    }
}
