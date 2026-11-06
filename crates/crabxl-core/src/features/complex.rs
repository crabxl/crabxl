use crate::{
    BinaryPayload, XmlElement,
    drawing::{PartRelationship, RelatedPart},
};

/// XLSX-family workbook kind, independent of the output filename.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkbookFormat {
    /// Ordinary workbook.
    #[default]
    Xlsx,
    /// Macro-enabled workbook.
    Xlsm,
    /// Workbook template.
    Xltx,
    /// Macro-enabled template.
    Xltm,
}
impl WorkbookFormat {
    /// Whether this workbook kind can contain a VBA project.
    pub const fn allows_macros(self) -> bool {
        matches!(self, Self::Xlsm | Self::Xltm)
    }
    /// Whether this workbook is a template.
    pub const fn is_template(self) -> bool {
        matches!(self, Self::Xltx | Self::Xltm)
    }
    /// Workbook-part content type.
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Xlsx => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
            }
            Self::Xlsm => "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
            Self::Xltx => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml"
            }
            Self::Xltm => "application/vnd.ms-excel.template.macroEnabled.main+xml",
        }
    }
    /// Resolve a workbook-part content type.
    pub fn from_content_type(value: &str) -> Option<Self> {
        [Self::Xlsx, Self::Xlsm, Self::Xltx, Self::Xltm]
            .into_iter()
            .find(|format| format.content_type() == value)
    }
}

/// External workbook, DDE or OLE link and its cached values and names.
/// External targets are retained as URIs and are never fetched.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalLink {
    /// Complete editable `externalLink` document.
    pub definition: XmlElement,
    /// Original part identity.
    pub source_part: Option<Box<str>>,
    /// Relationship identifiers referenced by the definition.
    pub relationships: Vec<PartRelationship>,
    /// Internal dependencies, if present.
    pub parts: Vec<RelatedPart>,
}
impl ExternalLink {
    /// Retained metadata and dependency allocation.
    pub fn heap_bytes(&self) -> usize {
        self.definition.heap_bytes()
            + self
                .parts
                .iter()
                .map(RelatedPart::heap_bytes)
                .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<PartRelationship>() + rel.id.len() + rel.kind.len() + rel.target.len()
                })
                .sum::<usize>()
    }
}

/// Workbook-owned cell/value metadata or rich-value graph root.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkbookMetadata {
    /// Full workbook relationship type URI.
    pub kind: Box<str>,
    /// Root-part content type.
    pub content_type: Box<str>,
    /// Complete editable root, including future metadata and extension subtrees.
    pub definition: XmlElement,
    /// Original package identity.
    pub source_part: Option<Box<str>>,
    /// Root-owned relationships.
    pub relationships: Vec<PartRelationship>,
    /// Related records, rich-value relations, media and other graph members.
    pub parts: Vec<RelatedPart>,
}
impl WorkbookMetadata {
    /// Retained metadata and dependency allocation.
    pub fn heap_bytes(&self) -> usize {
        self.definition.heap_bytes()
            + self
                .parts
                .iter()
                .map(RelatedPart::heap_bytes)
                .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<PartRelationship>() + rel.id.len() + rel.kind.len() + rel.target.len()
                })
                .sum::<usize>()
    }
    /// Actual cell/value metadata record counts; declared count attributes are not trusted.
    pub fn annotation_counts(&self) -> Option<(usize, usize)> {
        if self.kind.rsplit('/').next() != Some("sheetMetadata") {
            return None;
        }
        let count = |name| {
            self.definition.child(name).map_or(0, |container| container.children.iter().filter(|child| matches!(child, crate::XmlContent::Element(element) if element.local_name() == "bk")).count())
        };
        Some((count("cellMetadata"), count("valueMetadata")))
    }
}

/// VBA project and its signature or auxiliary dependencies.
#[derive(Clone, Debug, PartialEq)]
pub struct VbaProject {
    /// Compound binary project payload.
    pub data: BinaryPayload,
    /// Original part identity.
    pub source_part: Option<Box<str>>,
    /// Project-owned relationships.
    pub relationships: Vec<PartRelationship>,
    /// Project-owned dependent parts.
    pub parts: Vec<RelatedPart>,
}
impl VbaProject {
    /// Retained payload and dependency allocation.
    pub fn heap_bytes(&self) -> usize {
        self.data.memory_bytes()
            + self
                .parts
                .iter()
                .map(RelatedPart::heap_bytes)
                .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<PartRelationship>() + rel.id.len() + rel.kind.len() + rel.target.len()
                })
                .sum::<usize>()
    }
}

/// Explicit VBA policy. Preserving an original project does not execute it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MacroPolicy {
    /// Preserve an original project's bytes and graph; a new workbook has no project.
    #[default]
    Preserve,
    /// Remove the project and its owned dependencies.
    Remove,
    /// Install the supplied project graph.
    Replace(VbaProject),
}

/// Encoded printer settings and their package-owned dependencies.
#[derive(Clone, Debug, PartialEq)]
pub struct PrinterSettings {
    /// Relationship identity used by `PageSetup::printer_relationship`.
    pub relationship: Box<str>,
    /// Device-specific settings; CrabXL does not interpret or execute these bytes.
    pub data: crate::BinaryPayload,
    /// Original package root identity, when loaded.
    pub source_part: Option<Box<str>>,
    /// Root relationships.
    pub relationships: Vec<crate::drawing::PartRelationship>,
    /// Related immutable payloads.
    pub parts: Vec<crate::drawing::RelatedPart>,
}
impl PrinterSettings {
    /// Retained payload allocation.
    pub fn heap_bytes(&self) -> usize {
        self.relationship.len()
            + self.data.memory_bytes()
            + self.source_part.as_ref().map_or(0, |part| part.len())
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<crate::drawing::PartRelationship>()
                        + rel.id.len()
                        + rel.kind.len()
                        + rel.target.len()
                })
                .sum::<usize>()
            + self
                .parts
                .iter()
                .map(crate::drawing::RelatedPart::heap_bytes)
                .sum::<usize>()
    }
}
