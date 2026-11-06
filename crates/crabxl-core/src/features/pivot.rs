//! Pivot metadata adapted from umya-spreadsheet 3.1.0.
//! Copyright (c) 2020 MathNya. MIT; see third_party/licenses/umya-spreadsheet-MIT.txt.
use crate::{
    BinaryPayload, Error, ErrorKind, Result, XmlElement,
    drawing::{PartRelationship, RelatedPart},
};
use std::sync::Arc;

/// Pivot cache definition and its relationship-resolved record source.
#[derive(Clone, Debug, PartialEq)]
pub struct PivotCache {
    /// Workbook-local cache identity.
    pub id: u32,
    /// Complete editable `pivotCacheDefinition`, including shared items and extensions.
    pub definition: XmlElement,
    /// Original package identity; independent of conventional filenames.
    pub source_part: Option<Box<str>>,
    /// Relationships referenced by the definition.
    pub relationships: Vec<PartRelationship>,
    /// Cache records and other dependencies. Large records use disk-backed payloads.
    pub parts: Vec<RelatedPart>,
}
impl PivotCache {
    /// Attach an encoded record source. Record serialization can stream to a temporary file.
    pub fn set_records(&mut self, records: BinaryPayload, count: u32) {
        let retired: std::collections::HashSet<_> = self
            .relationships
            .iter()
            .filter(|rel| rel.kind.rsplit('/').next() == Some("pivotCacheRecords"))
            .map(|rel| rel.target.clone())
            .collect();
        self.relationships
            .retain(|rel| rel.kind.rsplit('/').next() != Some("pivotCacheRecords"));
        let retained: std::collections::HashSet<_> = self
            .relationships
            .iter()
            .chain(
                self.parts
                    .iter()
                    .filter(|part| !retired.contains(&part.name))
                    .flat_map(|part| part.relationships.iter()),
            )
            .filter(|rel| !rel.external)
            .map(|rel| rel.target.clone())
            .collect();
        self.parts
            .retain(|part| !retired.contains(&part.name) || retained.contains(&part.name));
        let mut target = String::from("crabxl/pivotCacheRecords.xml");
        while self.parts.iter().any(|part| part.name.as_ref() == target) {
            target.insert(target.len() - 4, '_');
        }
        self.definition
            .attributes
            .retain(|(name, _)| !(name.contains(':') && name.rsplit(':').next() == Some("id")));
        let strict =
            self.definition.attributes.iter().any(|(_, value)| {
                value.as_ref() == "http://purl.oclc.org/ooxml/spreadsheetml/main"
            });
        self.definition.set_attribute(
            "xmlns:r",
            if strict {
                "http://purl.oclc.org/ooxml/officeDocument/relationships"
            } else {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            },
        );
        let mut id = String::from("rIdRecords");
        while self.relationships.iter().any(|rel| rel.id.as_ref() == id) {
            id.push('_');
        }
        self.definition.set_attribute("r:id", id.as_str());
        self.definition
            .set_attribute("recordCount", count.to_string());
        self.relationships.push(PartRelationship { id: id.into(), kind: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords".into(), target: target.clone().into(), external: false });
        self.parts.push(RelatedPart {
            name: target.into(),
            content_type:
                "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml"
                    .into(),
            data: records,
            relationships: Vec::new(),
        });
    }
    /// Create a worksheet-backed cache with field names, without calculating pivot results.
    pub fn worksheet(
        id: u32,
        sheet: &str,
        range: crate::CellRange,
        fields: &[&str],
    ) -> Result<Self> {
        if fields.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Pivot cache needs fields",
            ));
        }
        let mut definition = XmlElement::new("pivotCacheDefinition");
        definition.set_attribute(
            "xmlns",
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        );
        definition.set_attribute(
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        );
        definition.set_attribute("refreshOnLoad", "1");
        definition.set_attribute("createdVersion", "3");
        definition.set_attribute("refreshedVersion", "3");
        definition.set_attribute("minRefreshableVersion", "3");
        let mut source = XmlElement::new("cacheSource");
        source.set_attribute("type", "worksheet");
        let mut worksheet = XmlElement::new("worksheetSource");
        worksheet.set_attribute("sheet", sheet);
        worksheet.set_attribute("ref", range.to_string());
        source.children.push(crate::XmlContent::Element(worksheet));
        let mut cache_fields = XmlElement::new("cacheFields");
        cache_fields.set_attribute("count", fields.len().to_string());
        for name in fields {
            let mut field = XmlElement::new("cacheField");
            field.set_attribute("name", *name);
            field
                .children
                .push(crate::XmlContent::Element(XmlElement::new("sharedItems")));
            cache_fields
                .children
                .push(crate::XmlContent::Element(field));
        }
        definition.children.extend([
            crate::XmlContent::Element(source),
            crate::XmlContent::Element(cache_fields),
        ]);
        Ok(Self {
            id,
            definition,
            source_part: None,
            relationships: Vec::new(),
            parts: Vec::new(),
        })
    }
    /// Encoded cache-record source, when present. Access does not materialize records.
    pub fn records(&self) -> Option<&BinaryPayload> {
        let target = self
            .relationships
            .iter()
            .find(|rel| !rel.external && rel.kind.rsplit('/').next() == Some("pivotCacheRecords"))?
            .target
            .as_ref();
        self.parts
            .iter()
            .find(|part| part.name.as_ref() == target)
            .map(|part| &part.data)
    }
    /// Retained metadata and payload allocation.
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

/// A pivot table and a shared immutable cache. Use `Arc::make_mut` for isolated cache edits.
#[derive(Clone, Debug, PartialEq)]
pub struct PivotTable {
    /// Complete editable table definition: location, fields, filters, styles and settings.
    pub definition: XmlElement,
    /// Cache shared with other pivot tables in this model.
    pub cache: Arc<PivotCache>,
    /// Table-owned relationships, including its cache edge.
    pub relationships: Vec<PartRelationship>,
    /// Table-owned dependencies other than the separately shared cache.
    pub parts: Vec<RelatedPart>,
    /// Original part identity, if loaded.
    pub source_part: Option<Box<str>>,
}
impl PivotTable {
    /// Create a pivot layout over an existing cache. Result calculation is delegated to Excel.
    pub fn new(name: &str, location: crate::CellRange, cache: Arc<PivotCache>) -> Result<Self> {
        if name.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Pivot table name is empty",
            ));
        }
        let mut definition = XmlElement::new("pivotTableDefinition");
        definition.set_attribute(
            "xmlns",
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        );
        definition.set_attribute("name", name);
        definition.set_attribute("cacheId", cache.id.to_string());
        definition.set_attribute("dataCaption", "Values");
        let mut element = XmlElement::new("location");
        element.set_attribute("ref", location.to_string());
        for key in ["firstHeaderRow", "firstDataRow", "firstDataCol"] {
            element.set_attribute(key, "1");
        }
        definition
            .children
            .push(crate::XmlContent::Element(element));
        let count = cache.definition.child("cacheFields").map_or(0, |fields| fields.children.iter().filter(|field| matches!(field, crate::XmlContent::Element(element) if element.local_name() == "cacheField")).count());
        let mut fields = XmlElement::new("pivotFields");
        fields.set_attribute("count", count.to_string());
        for _ in 0..count {
            let mut field = XmlElement::new("pivotField");
            field.set_attribute("showAll", "0");
            fields.children.push(crate::XmlContent::Element(field));
        }
        definition.children.push(crate::XmlContent::Element(fields));
        let relationships = vec![PartRelationship { id: "rIdCache".into(), kind: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition".into(), target: cache.source_part.clone().unwrap_or_else(|| "crabxl/cache".into()), external: false }];
        Ok(Self {
            definition,
            cache,
            relationships,
            parts: Vec::new(),
            source_part: None,
        })
    }
    /// Retained metadata and cache payload allocation.
    pub fn heap_bytes(&self) -> usize {
        self.definition.heap_bytes()
            + self.cache.heap_bytes()
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

/// One typed pivot-cache record value.
#[derive(Clone, Debug, PartialEq)]
pub enum PivotValue {
    /// Missing item (`m`).
    Missing,
    /// Numeric item (`n`).
    Number(f64),
    /// Boolean item (`b`).
    Boolean(bool),
    /// Error literal (`e`).
    Error(Box<str>),
    /// String item (`s`).
    Text(Box<str>),
    /// ISO date/time literal (`d`).
    DateTime(Box<str>),
    /// Shared-item index (`x`).
    Index(u32),
    /// Tuple or extended item retained as an editable element.
    Extended(XmlElement),
}
impl PivotValue {
    /// Decode a record item. Additional attributes or children retain the whole element.
    pub fn from_element(element: XmlElement) -> Result<Self> {
        if element.attributes.iter().any(|(name, _)| {
            name.as_ref() != "v" && name.as_ref() != "xmlns" && !name.starts_with("xmlns:")
        }) || !element.children.is_empty()
        {
            return Ok(Self::Extended(element));
        }
        let value = element.attribute("v").unwrap_or("");
        let invalid = || Error::new(ErrorKind::InvalidData, "Invalid pivot cache record value");
        match element.local_name() {
            "m" => Ok(Self::Missing),
            "n" => Ok(Self::Number(value.parse().map_err(|_| invalid())?)),
            "b" => match value {
                "1" | "true" => Ok(Self::Boolean(true)),
                "0" | "false" => Ok(Self::Boolean(false)),
                _ => Err(invalid()),
            },
            "e" => Ok(Self::Error(value.into())),
            "s" => Ok(Self::Text(value.into())),
            "d" => Ok(Self::DateTime(value.into())),
            "x" => Ok(Self::Index(value.parse().map_err(|_| invalid())?)),
            _ => Ok(Self::Extended(element)),
        }
    }
    /// Convert to a SpreadsheetML cache-record item.
    pub fn to_element(&self) -> XmlElement {
        let (name, value) = match self {
            Self::Missing => return XmlElement::new("m"),
            Self::Extended(element) => return element.clone(),
            Self::Number(value) => ("n", value.to_string()),
            Self::Boolean(value) => ("b", u8::from(*value).to_string()),
            Self::Error(value) => ("e", value.to_string()),
            Self::Text(value) => ("s", value.to_string()),
            Self::DateTime(value) => ("d", value.to_string()),
            Self::Index(value) => ("x", value.to_string()),
        };
        let mut element = XmlElement::new(name);
        element.set_attribute("v", value);
        element
    }
}
