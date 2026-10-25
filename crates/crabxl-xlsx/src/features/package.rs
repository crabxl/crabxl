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
}
fn kind_is(kind: &str, suffix: &str) -> bool {
    kind.rsplit('/').next() == Some(suffix)
}

impl<R: Read + Seek> WorkbookReader<R> {
    fn sheet_part(&self, name: &str) -> Result<String> {
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
        if info.kind() != crate::SheetKind::Worksheet {
            return Err(unsupported("Features require a cell worksheet"));
        }
        Ok(info.part().to_owned())
    }

    fn part_stream(
        &mut self,
        part: &str,
    ) -> Result<XmlStream<BufReader<zip::read::ZipFile<'_, R>>>> {
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
            self.limits.max_part_bytes,
            self.limits,
        ))
    }

    fn part_bytes(&mut self, part: &str) -> Result<Vec<u8>> {
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
        let (mut features, refs) = {
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
                return Err(unsupported("External feature parts are not supported"));
            }
            crate::package::resolve_part(&part, &relationship.target)
        };
        for id in &refs.tables {
            sources.tables.push((id.clone(), target(id)?));
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
        Ok((features, sources))
    }

    /// Read defined names, workbook protection and document properties.
    pub fn workbook_features(&mut self) -> Result<WorkbookFeatures> {
        let mut features = WorkbookFeatures::default();
        let workbook_part = self.workbook_part.clone();
        {
            let mut xml = self.part_stream(&workbook_part)?;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e) if frame.depth == 2 && frame.scope == Scope::Spreadsheet => {
                        let e = e.into_owned();
                        match e.local_name().as_ref().as_bytes() {
                            b"definedNames" => {
                                features.defined_names =
                                    super::defined_name::read(&mut xml, &e, 2)?;
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
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// A new worksheet relationship.
#[derive(Clone, Debug)]
pub(crate) struct OutputRelationship {
    pub id: String,
    pub kind: &'static str,
    pub target: String,
}
impl OutputRelationship {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(b"<Relationship Id=\"")?;
        out.write_all(quick_xml::escape::escape(self.id.as_str()).as_bytes())?;
        write!(out, "\" Type=\"{}\" Target=\"", self.kind)?;
        out.write_all(quick_xml::escape::escape(self.target.as_str()).as_bytes())?;
        out.write_all(b"\"/>")
    }
}

/// Feature parts and relationships for one worksheet.
#[derive(Clone, Debug, Default)]
pub(crate) struct SheetOutput {
    pub refs: PartRefs,
    pub parts: Vec<OutputPart>,
    pub relationships: Vec<OutputRelationship>,
}
impl SheetOutput {
    pub(crate) fn heap_bytes(&self) -> usize {
        self.parts
            .iter()
            .map(|part| part.name.capacity() + part.bytes.capacity() + 64)
            .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| rel.id.capacity() + rel.target.capacity() + 64)
                .sum::<usize>()
    }
}

/// Part-number allocator that keeps new part names and table ids unique.
#[derive(Clone, Debug)]
pub(crate) struct PartNumbers {
    pub table_file: u32,
    pub table_id: u32,
    pub comments_file: u32,
    pub vml_file: u32,
    taken: std::collections::HashSet<String>,
}
impl Default for PartNumbers {
    fn default() -> Self {
        Self {
            table_file: 1,
            table_id: 1,
            comments_file: 1,
            vml_file: 1,
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
        self.table_id = self.table_id.max(id.saturating_add(1));
    }
    fn next_name(&mut self, counter: fn(&mut Self) -> &mut u32, pattern: (&str, &str)) -> String {
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
fn relative_target(sheet_part: &str, part: &str) -> String {
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
    for table in &features.tables {
        let name = numbers.next_name(|n| &mut n.table_file, ("xl/tables/table", ".xml"));
        let id = numbers.table_id;
        numbers.table_id += 1;
        let mut bytes = Vec::new();
        super::table::write_part(&mut bytes, table, id)?;
        let rid = relationship_id("table");
        output.relationships.push(OutputRelationship {
            id: rid.clone(),
            kind: REL_TABLE,
            target: relative_target(sheet_part, &name),
        });
        output.refs.tables.push(rid);
        output.parts.push(OutputPart {
            name,
            content_type: CT_TABLE,
            bytes,
        });
    }
    if !features.comments.is_empty() {
        let name = numbers.next_name(|n| &mut n.comments_file, ("xl/comments/comment", ".xml"));
        let mut bytes = Vec::new();
        super::comment::write_part(&mut bytes, &features.comments)?;
        output.relationships.push(OutputRelationship {
            id: relationship_id("comments"),
            kind: REL_COMMENTS,
            target: relative_target(sheet_part, &name),
        });
        output.parts.push(OutputPart {
            name,
            content_type: CT_COMMENTS,
            bytes,
        });
        let name = numbers.next_name(|n| &mut n.vml_file, ("xl/drawings/commentsDrawing", ".vml"));
        let mut bytes = Vec::new();
        super::comment::write_vml(&mut bytes, &features.comments, sheet_index)?;
        let rid = relationship_id("vml");
        output.relationships.push(OutputRelationship {
            id: rid.clone(),
            kind: REL_VML,
            target: relative_target(sheet_part, &name),
        });
        output.refs.legacy_drawing = Some(rid);
        output.parts.push(OutputPart {
            name,
            content_type: CT_VML,
            bytes,
        });
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
