//! Workbook-level external-link, VBA and template graph handling.
use crate::xml::Scope;
use crabxl_core::{
    Result,
    features::{ExternalLink, VbaProject, WorkbookMetadata},
};
use quick_xml::events::Event;
use std::io::{Read, Seek};

fn invalid(message: &str) -> crabxl_core::Error {
    crate::features::invalid(message)
}

impl<R: Read + Seek> crate::WorkbookReader<R> {
    pub(crate) fn annotation_counts(&mut self) -> Result<(usize, usize)> {
        let workbook = self.workbook_part.clone();
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &crate::package::relationship_part(&workbook),
            self.limits,
            &mut remaining,
        )?;
        let mut roots = relationships
            .values()
            .filter(|rel| rel.kind.rsplit('/').next() == Some("sheetMetadata"));
        let Some(root) = roots.next() else {
            return Ok((0, 0));
        };
        if root.external || roots.next().is_some() {
            return Err(invalid("Invalid sheet metadata ownership"));
        }
        let part = crate::package::resolve_part(&workbook, &root.target)?;
        let definition = self.metadata_element(&part, "metadata")?;
        let count = |name| {
            definition.child(name).map_or(0, |container| container.children.iter().filter(|child| matches!(child, crabxl_core::XmlContent::Element(element) if element.local_name() == "bk")).count())
        };
        Ok((count("cellMetadata"), count("valueMetadata")))
    }
    /// External links in workbook-reference order. Cached values are metadata, not recalculated values.
    pub fn external_links(&mut self) -> Result<Vec<ExternalLink>> {
        let workbook = self.workbook_part.clone();
        let mut ids = Vec::new();
        {
            let mut xml = self.part_stream(&workbook)?;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e)
                        if frame.depth == 3
                            && frame.scope == Scope::Spreadsheet
                            && e.local_name().as_ref() == "externalReference" =>
                    {
                        ids.push(
                            frame
                                .office_relationship
                                .ok_or_else(|| invalid("External reference has no relationship"))?
                                .to_owned(),
                        )
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &crate::package::relationship_part(&workbook),
            self.limits,
            &mut remaining,
        )?;
        let mut output = Vec::with_capacity(ids.len());
        for id in ids {
            let rel = relationships
                .get(&id)
                .ok_or_else(|| invalid("Missing external-link relationship"))?;
            if rel.external || rel.kind.rsplit('/').next() != Some("externalLink") {
                return Err(invalid("Invalid external-link relationship"));
            }
            let part = crate::package::resolve_part(&workbook, &rel.target)?;
            let definition = self.metadata_element(&part, "externalLink")?;
            let (relationships, parts) = self.read_part_dependencies(&part)?;
            output.push(ExternalLink {
                definition,
                source_part: Some(part.into()),
                relationships,
                parts,
            });
        }
        Ok(output)
    }

    /// Cell/value metadata and rich-value roots, with original relationship identities.
    pub fn workbook_metadata(&mut self) -> Result<Vec<WorkbookMetadata>> {
        let workbook = self.workbook_part.clone();
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &crate::package::relationship_part(&workbook),
            self.limits,
            &mut remaining,
        )?;
        let mut roots: Vec<_> = relationships
            .into_iter()
            .filter(|(_, rel)| metadata_relationship(&rel.kind))
            .collect();
        roots.sort_by(|a, b| a.0.cmp(&b.0));
        let mut output = Vec::new();
        for (_, rel) in roots {
            if rel.external {
                return Err(invalid("Workbook metadata root cannot be external"));
            }
            let part = crate::package::resolve_part(&workbook, &rel.target)?;
            let content_type = self.part_content_type(&part)?;
            let mut xml = self.part_stream(&part)?;
            let mut definition = None;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e) if frame.depth == 1 => {
                        let e = e.into_owned();
                        definition = Some(crate::drawing_xml::read(&mut xml, &e, 1)?);
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
            let definition = definition.ok_or_else(|| invalid("Missing workbook metadata root"))?;
            drop(xml);
            let (relationships, parts) = self.read_part_dependencies(&part)?;
            output.push(WorkbookMetadata {
                kind: rel.kind.into(),
                content_type: content_type.into(),
                definition,
                source_part: Some(part.into()),
                relationships,
                parts,
            });
        }
        Ok(output)
    }
    fn part_content_type(&mut self, part: &str) -> Result<String> {
        let mut xml = self.part_stream("[Content_Types].xml")?;
        let mut value = None;
        let extension = part.rsplit('.').next().unwrap_or("");
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e) if frame.depth == 2 && frame.scope == Scope::ContentTypes => {
                    let name = e.local_name();
                    if name.as_ref() == "Override"
                        && crate::package::resolve_part(
                            "",
                            &crate::xml::required_attribute(&e, b"PartName")?,
                        )? == part
                    {
                        return crate::xml::required_attribute(&e, b"ContentType");
                    }
                    if name.as_ref() == "Default"
                        && crate::xml::required_attribute(&e, b"Extension")?
                            .eq_ignore_ascii_case(extension)
                    {
                        value = Some(crate::xml::required_attribute(&e, b"ContentType")?);
                    }
                }
                Event::Eof => return value.ok_or_else(|| invalid("Part has no content type")),
                _ => {}
            }
        }
    }
    /// Original VBA project, if present. This only reads encoded bytes and never executes code.
    pub fn vba_project(&mut self) -> Result<Option<VbaProject>> {
        let workbook = self.workbook_part.clone();
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &crate::package::relationship_part(&workbook),
            self.limits,
            &mut remaining,
        )?;
        let mut projects = relationships
            .values()
            .filter(|rel| rel.kind.rsplit('/').next() == Some("vbaProject"));
        let Some(project) = projects.next() else {
            return Ok(None);
        };
        if project.external || projects.next().is_some() {
            return Err(invalid("Invalid VBA project ownership"));
        }
        let part = crate::package::resolve_part(&workbook, &project.target)?;
        let data = self.part_payload(&part)?;
        let (relationships, parts) = self.read_part_dependencies(&part)?;
        Ok(Some(VbaProject {
            data,
            source_part: Some(part.into()),
            relationships,
            parts,
        }))
    }
}

pub(crate) fn metadata_relationship(kind: &str) -> bool {
    matches!(
        kind.rsplit('/').next(),
        Some(
            "sheetMetadata"
                | "rdRichValue"
                | "rdRichValueStructure"
                | "rdRichValueTypes"
                | "richValueRel"
                | "richValue"
                | "richValueStructure"
                | "richValueTypes"
                | "connections"
        )
    )
}
pub(crate) const REL_EXTERNAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink";
pub(crate) const REL_VBA: &str =
    "http://schemas.microsoft.com/office/2006/relationships/vbaProject";
const CT_EXTERNAL: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml";
const CT_VBA: &str = "application/vnd.ms-office.vbaProject";

#[derive(Default)]
pub(crate) struct WorkbookOutput {
    pub parts: Vec<crate::features::package::OutputPart>,
    pub relationships: Vec<crate::features::package::OutputRelationship>,
    pub external_refs: Vec<String>,
}

pub(crate) fn build_output(
    features: &crabxl_core::WorkbookFeatures,
    workbook: &str,
    numbers: &mut crate::features::package::PartNumbers,
    mut relationship_id: impl FnMut(&str) -> String,
) -> std::io::Result<WorkbookOutput> {
    use crate::features::package::{OutputPart, OutputRelationship, SheetOutput, relative_target};
    use crabxl_core::features::MacroPolicy;
    let mut output = WorkbookOutput::default();
    let mut dependencies = SheetOutput::default();
    if let Some(links) = &features.external_links {
        for link in links {
            if link.definition.local_name() != "externalLink" {
                return Err(std::io::Error::other("Invalid external-link document"));
            }
            let ids = link
                .relationships
                .iter()
                .map(|rel| rel.id.as_ref())
                .collect();
            crate::drawing::chart_relationships(&link.definition, &ids)
                .map_err(std::io::Error::other)?;
            let part = numbers.next_name(
                |n| &mut n.external_file,
                ("xl/externalLinks/externalLink", ".xml"),
            );
            let mut bytes = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_vec();
            crate::drawing_xml::write(&mut bytes, &link.definition, 0)?;
            output.parts.push(OutputPart {
                name: part.clone(),
                content_type: CT_EXTERNAL.into(),
                bytes: bytes.into(),
            });
            crate::drawing::build_part_dependencies(
                &link.relationships,
                &link.parts,
                link.source_part.as_deref(),
                &part,
                numbers,
                &mut dependencies,
            )?;
            let id = relationship_id("externalLink");
            output.external_refs.push(id.clone());
            output.relationships.push(OutputRelationship {
                id,
                kind: REL_EXTERNAL.into(),
                target: relative_target(workbook, &part),
            });
        }
    }
    if let Some(roots) = &features.metadata {
        let mut kinds = std::collections::HashSet::new();
        for root in roots {
            if !metadata_relationship(&root.kind) || !kinds.insert(root.kind.rsplit('/').next()) {
                return Err(std::io::Error::other(
                    "Unknown workbook metadata relationship type",
                ));
            }
            let ids = root
                .relationships
                .iter()
                .map(|rel| rel.id.as_ref())
                .collect();
            crate::drawing::chart_relationships(&root.definition, &ids)
                .map_err(std::io::Error::other)?;
            let part = numbers.next_name(|n| &mut n.metadata_file, ("xl/metadata/part", ".xml"));
            let mut bytes = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_vec();
            crate::drawing_xml::write(&mut bytes, &root.definition, 0)?;
            output.parts.push(OutputPart {
                name: part.clone(),
                content_type: root.content_type.clone(),
                bytes: bytes.into(),
            });
            crate::drawing::build_part_dependencies(
                &root.relationships,
                &root.parts,
                root.source_part.as_deref(),
                &part,
                numbers,
                &mut dependencies,
            )?;
            output.relationships.push(OutputRelationship {
                id: relationship_id("metadata"),
                kind: root.kind.clone(),
                target: relative_target(workbook, &part),
            });
        }
    }
    if let MacroPolicy::Replace(project) = &features.macros {
        if project.data.is_empty() {
            return Err(std::io::Error::other("Empty VBA project"));
        }
        let part = numbers.next_name(|n| &mut n.vba_file, ("xl/vbaProject", ".bin"));
        crate::drawing::build_part_dependencies(
            &project.relationships,
            &project.parts,
            project.source_part.as_deref(),
            &part,
            numbers,
            &mut dependencies,
        )?;
        output.parts.push(OutputPart {
            name: part.clone(),
            content_type: CT_VBA.into(),
            bytes: project.data.clone(),
        });
        output.relationships.push(OutputRelationship {
            id: relationship_id("vbaProject"),
            kind: REL_VBA.into(),
            target: relative_target(workbook, &part),
        });
    }
    output.parts.append(&mut dependencies.parts);
    Ok(output)
}

pub(crate) fn write_external_refs(
    out: &mut impl std::io::Write,
    refs: &[String],
    uri: Option<&str>,
) -> std::io::Result<()> {
    if refs.is_empty() {
        return Ok(());
    }
    out.write_all(b"<externalReferences")?;
    crate::printing::namespace(out, uri)?;
    out.write_all(b">")?;
    let rel = if uri == Some(crate::xml::STRICT_MAIN_URI) {
        crate::xml::STRICT_OFFICE_REL_URI
    } else {
        crate::xml::OFFICE_REL_URI
    };
    for id in refs {
        write!(
            out,
            "<externalReference xmlns:r=\"{rel}\" r:id=\"{}\"/>",
            quick_xml::escape::escape(id)
        )?;
    }
    out.write_all(b"</externalReferences>")
}
