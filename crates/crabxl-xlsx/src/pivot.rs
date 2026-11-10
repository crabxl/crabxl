//! Pivot definitions and bounded cache-record iteration.
use crate::xml::{Scope, XmlStream, required_attribute};
use crabxl_core::{
    BinaryReader, Result, XmlContent, XmlElement,
    features::{PivotCache, PivotTable, PivotValue},
};
use quick_xml::events::Event;
use std::{
    collections::HashMap,
    io::{BufReader, Read, Seek},
    sync::Arc,
};

fn invalid(message: &str) -> crabxl_core::Error {
    crate::features::invalid(message)
}

pub(crate) fn implicit_pivot_ids(
    relationships: &HashMap<String, crate::package::Relationship>,
) -> Vec<String> {
    let mut ids: Vec<_> = relationships
        .iter()
        .filter(|(_, rel)| rel.kind.rsplit('/').next() == Some("pivotTable"))
        .map(|(id, _)| id.clone())
        .collect();
    ids.sort_unstable();
    ids
}

impl<R: Read + Seek> crate::WorkbookReader<R> {
    pub(crate) fn metadata_element(&mut self, part: &str, expected: &str) -> Result<XmlElement> {
        let mut xml = self.part_stream(part)?;
        let mut value = None;
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.depth == 1
                        && frame.scope == Scope::Spreadsheet
                        && e.local_name().as_ref() == expected =>
                {
                    let e = e.into_owned();
                    value = Some(crate::drawing_xml::read(&mut xml, &e, 1)?);
                }
                Event::Start(_) => {
                    return Err(invalid("Invalid metadata part root").with_part(part));
                }
                Event::Eof => {
                    return value
                        .ok_or_else(|| invalid("Missing metadata part root").with_part(part));
                }
                _ => {}
            }
        }
    }

    /// Read pivot tables and cache metadata. Cache record payloads stay file-backed when large.
    pub fn sheet_pivots(&mut self, sheet: &str) -> Result<Vec<PivotTable>> {
        let sheet_part = self.sheet_part(sheet)?;
        let mut ids = Vec::new();
        {
            let mut xml = self.part_stream(&sheet_part)?;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e)
                        if frame.depth == 3
                            && frame.scope == Scope::Spreadsheet
                            && e.local_name().as_ref() == "pivotTablePart" =>
                    {
                        ids.push(
                            frame
                                .office_relationship
                                .ok_or_else(|| invalid("Pivot table has no relationship"))?
                                .to_owned(),
                        )
                    }
                    Event::Start(e)
                        if frame.depth == 2
                            && frame.scope == Scope::Spreadsheet
                            && e.local_name().as_ref() == "sheetData" =>
                    {
                        crate::style_codec::skip(&mut xml, 2)?;
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
        let rels_part = crate::package::relationship_part(&sheet_part);
        if ids.is_empty() && self.archive.index_for_name(&rels_part).is_none() {
            return Ok(Vec::new());
        }
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &rels_part,
            self.limits,
            &mut remaining,
        )?;
        if ids.is_empty() {
            ids = implicit_pivot_ids(&relationships);
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let workbook = self.workbook_part.clone();
        let book_relationships = crate::package::read_relationships(
            &mut self.archive,
            &crate::package::relationship_part(&workbook),
            self.limits,
            &mut remaining,
        )?;
        let mut cache_parts = HashMap::new();
        {
            let mut xml = self.part_stream(&workbook)?;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e)
                        if frame.depth == 3
                            && frame.scope == Scope::Spreadsheet
                            && e.local_name().as_ref() == "pivotCache" =>
                    {
                        let id: u32 = required_attribute(&e, b"cacheId")?
                            .parse()
                            .map_err(|_| invalid("Invalid pivot cache id"))?;
                        let rid = frame
                            .office_relationship
                            .ok_or_else(|| invalid("Pivot cache has no relationship"))?;
                        let rel = book_relationships
                            .get(&rid)
                            .ok_or_else(|| invalid("Missing pivot cache relationship"))?;
                        if rel.external
                            || rel.kind.rsplit('/').next() != Some("pivotCacheDefinition")
                        {
                            return Err(invalid("Invalid pivot cache relationship"));
                        }
                        if cache_parts
                            .insert(id, crate::package::resolve_part(&workbook, &rel.target)?)
                            .is_some()
                        {
                            return Err(invalid("Duplicate pivot cache id"));
                        }
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
        let mut caches = HashMap::new();
        let mut tables = Vec::new();
        for id in ids {
            let rel = relationships
                .get(&id)
                .ok_or_else(|| invalid("Missing pivot table relationship"))?;
            if rel.external || rel.kind.rsplit('/').next() != Some("pivotTable") {
                return Err(invalid("Invalid pivot table relationship"));
            }
            let part = crate::package::resolve_part(&sheet_part, &rel.target)?;
            let definition = self.metadata_element(&part, "pivotTableDefinition")?;
            let cache_id: u32 = definition
                .attribute("cacheId")
                .ok_or_else(|| invalid("Pivot table has no cache id"))?
                .parse()
                .map_err(|_| invalid("Invalid pivot table cache id"))?;
            if let std::collections::hash_map::Entry::Vacant(entry) = caches.entry(cache_id) {
                let source = cache_parts
                    .get(&cache_id)
                    .ok_or_else(|| invalid("Pivot cache declaration is missing"))?;
                let definition = self.metadata_element(source, "pivotCacheDefinition")?;
                let (relationships, parts) = self.read_part_dependencies(source)?;
                entry.insert(Arc::new(PivotCache {
                    id: cache_id,
                    definition,
                    source_part: Some(source.clone().into()),
                    relationships,
                    parts,
                }));
            }
            let cache = caches
                .get(&cache_id)
                .ok_or_else(|| invalid("Missing loaded pivot cache"))?
                .clone();
            let cache_source = cache
                .source_part
                .as_deref()
                .ok_or_else(|| invalid("Missing pivot cache source identity"))?;
            let (relationships, parts) =
                self.read_part_dependencies_without(&part, &[cache_source])?;
            let edges: Vec<_> = relationships
                .iter()
                .filter(|rel| rel.kind.rsplit('/').next() == Some("pivotCacheDefinition"))
                .collect();
            if edges.len() != 1 || edges[0].external || edges[0].target.as_ref() != cache_source {
                return Err(invalid(
                    "Pivot table cache relationship disagrees with its workbook declaration",
                ));
            }
            tables.push(PivotTable {
                relationships,
                parts,
                definition,
                cache,
                source_part: Some(part.into()),
            });
        }
        Ok(tables)
    }
}

/// One-record-at-a-time reader over a pivot cache's immutable encoded record source.
pub struct PivotRecords<'a> {
    xml: XmlStream<BufReader<BinaryReader<'a>>>,
    limits: crabxl_core::ResourceLimits,
    root: Option<XmlElement>,
    done: bool,
    fields: usize,
    shared_items: Vec<usize>,
    declared: Option<u64>,
    read: u64,
}
impl<'a> PivotRecords<'a> {
    /// Open the record source without copying or decoding the full cache.
    pub fn new(cache: &'a PivotCache, limits: crabxl_core::ResourceLimits) -> Result<Self> {
        let data = cache
            .records()
            .ok_or_else(|| invalid("Pivot cache has no records"))?;
        let fields = cache.definition.child("cacheFields").map_or(0, |fields| fields.children.iter().filter(|field| matches!(field, XmlContent::Element(element) if element.local_name() == "cacheField")).count());
        let shared_items = cache.definition.child("cacheFields").into_iter().flat_map(|fields| fields.children.iter()).filter_map(|field| match field { XmlContent::Element(field) if field.local_name() == "cacheField" => Some(field.child("sharedItems").map_or(0, |items| items.children.iter().filter(|item| matches!(item, XmlContent::Element(element) if matches!(element.local_name(), "m" | "n" | "b" | "e" | "s" | "d"))).count())), _ => None }).collect();
        Ok(Self {
            xml: XmlStream::new(
                BufReader::with_capacity(limits.input_buffer_bytes, data.reader()),
                "pivotCacheRecords".into(),
                limits.max_part_bytes,
                limits,
            ),
            limits,
            root: None,
            done: false,
            fields,
            shared_items,
            declared: None,
            read: 0,
        })
    }
    /// Read one record; limits apply to actual record size, not declared counts.
    pub fn next_record(&mut self) -> Result<Option<Vec<PivotValue>>> {
        if self.done {
            return Ok(None);
        }
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e) if frame.depth == 1 => {
                    if self.root.is_some()
                        || frame.scope != Scope::Spreadsheet
                        || e.local_name().as_ref() != "pivotCacheRecords"
                    {
                        return Err(invalid("Invalid pivot records root"));
                    }
                    self.declared = crate::xml::attribute(&e, b"count")?
                        .map(|value| {
                            value
                                .parse()
                                .map_err(|_| invalid("Invalid pivot record count"))
                        })
                        .transpose()?;
                    self.root = Some(crate::drawing_xml::element(&e)?);
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Spreadsheet
                        && e.local_name().as_ref() == "r" =>
                {
                    let mut parent = crate::drawing_xml::element(&e)?;
                    if let Some(root) = &self.root {
                        crate::drawing_xml::inherit(&mut parent, root);
                    }
                    let record = self.record(parent)?;
                    self.read += 1;
                    return Ok(Some(record));
                }
                Event::Start(_) => {
                    let depth = frame.depth;
                    crate::style_codec::skip(&mut self.xml, depth)?;
                }
                Event::Eof => {
                    self.done = true;
                    if self.root.is_none() {
                        return Err(invalid("Missing pivot records root"));
                    }
                    if self.declared.is_some_and(|count| count != self.read) {
                        return Err(invalid("Pivot record count differs from its declaration"));
                    }
                    return Ok(None);
                }
                _ => {}
            }
        }
    }
    fn record(&mut self, parent: XmlElement) -> Result<Vec<PivotValue>> {
        let mut values = Vec::new();
        let mut bytes = 0usize;
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e) if frame.depth == 3 && frame.scope == Scope::Spreadsheet => {
                    if values.len() >= self.limits.max_row_cells {
                        return Err(crate::features::invalid(
                            "Pivot record field limit exceeded",
                        ));
                    }
                    let e = e.into_owned();
                    let mut remaining = self.limits.max_row_bytes.saturating_sub(bytes);
                    let mut element =
                        crate::drawing_xml::read_limited(&mut self.xml, &e, 3, &mut remaining)?;
                    crate::drawing_xml::inherit(&mut element, &parent);
                    bytes = bytes
                        .saturating_add(element.heap_bytes())
                        .saturating_add(size_of::<PivotValue>());
                    if bytes > self.limits.max_row_bytes {
                        return Err(crabxl_core::Error::new(
                            crabxl_core::ErrorKind::LimitExceeded,
                            "Pivot record byte limit exceeded",
                        ));
                    }
                    let value = PivotValue::from_element(element)?;
                    if matches!(&value, PivotValue::Number(value) if !value.is_finite())
                        || matches!(&value, PivotValue::Index(index) if self.shared_items.get(values.len()).is_none_or(|count| *index as usize >= *count))
                    {
                        return Err(invalid(
                            "Invalid pivot record number or shared-item reference",
                        ));
                    }
                    values.push(value);
                }
                Event::Start(_) => return Err(invalid("Invalid pivot record item")),
                Event::End(_) if frame.depth == 1 => {
                    if self.fields != 0 && values.len() != self.fields {
                        return Err(invalid(
                            "Pivot record field count differs from cache fields",
                        ));
                    }
                    return Ok(values);
                }
                Event::Eof => return Err(invalid("Truncated pivot record")),
                _ => {}
            }
        }
    }
}

/// Encode cache records to an owned disk source using one record at a time.
pub fn write_pivot_records(
    records: impl IntoIterator<Item = Vec<PivotValue>>,
    limits: crabxl_core::ResourceLimits,
) -> Result<(crabxl_core::BinaryPayload, u32)> {
    use std::io::{SeekFrom, Write};
    let error = |error| {
        crabxl_core::Error::caused_by(
            crabxl_core::ErrorKind::Io,
            "Cannot spool pivot cache records",
            error,
        )
    };
    let mut file = tempfile::tempfile().map_err(error)?;
    let prefix = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><pivotCacheRecords xmlns=\"{}\" count=\"",
        crate::xml::MAIN_URI
    );
    file.write_all(prefix.as_bytes()).map_err(error)?;
    file.write_all(b"0000000000\">").map_err(error)?;
    let mut count = 0u32;
    let mut fields = None;
    for record in records {
        if record.len() > limits.max_row_cells {
            return Err(invalid("Pivot record field limit exceeded"));
        }
        if fields.is_some_and(|fields| fields != record.len()) {
            return Err(invalid("Inconsistent pivot record field counts"));
        }
        fields = Some(record.len());
        let mut bytes = crate::encode::RowBuffer {
            data: Vec::new(),
            maximum: limits.max_row_bytes,
        };
        bytes.write_all(b"<r>").map_err(error)?;
        for value in record {
            if matches!(value, PivotValue::Number(value) if !value.is_finite()) {
                return Err(invalid("Nonfinite pivot cache number"));
            }
            let estimate = match &value {
                PivotValue::Extended(element) => element.heap_bytes(),
                PivotValue::Text(value)
                | PivotValue::DateTime(value)
                | PivotValue::Error(value) => value.len(),
                _ => 0,
            };
            if estimate > limits.max_row_bytes.saturating_sub(bytes.data.len()) {
                return Err(invalid("Pivot record byte limit exceeded"));
            }
            crate::drawing_xml::write(&mut bytes, &value.to_element(), 0).map_err(error)?;
        }
        bytes.write_all(b"</r>").map_err(error)?;
        file.write_all(&bytes.data).map_err(error)?;
        if file.stream_position().map_err(error)? > limits.max_part_bytes {
            return Err(crabxl_core::Error::new(
                crabxl_core::ErrorKind::LimitExceeded,
                "Pivot record part byte limit exceeded",
            ));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| invalid("Pivot record count overflow"))?;
    }
    file.write_all(b"</pivotCacheRecords>").map_err(error)?;
    if file.stream_position().map_err(error)? > limits.max_part_bytes {
        return Err(crabxl_core::Error::new(
            crabxl_core::ErrorKind::LimitExceeded,
            "Pivot record part byte limit exceeded",
        ));
    }
    file.seek(SeekFrom::Start(prefix.len() as u64))
        .map_err(error)?;
    write!(file, "{count:010}").map_err(error)?;
    Ok((
        crabxl_core::BinaryPayload::from_file(file).map_err(error)?,
        count,
    ))
}

pub(crate) const REL_CACHE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition";
const REL_PIVOT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable";
const CT_PIVOT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml";
const CT_CACHE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";

fn excel_cache_definition(definition: &XmlElement) -> std::borrow::Cow<'_, XmlElement> {
    let needs_items = |field: &XmlElement| {
        field.local_name() == "cacheField"
            && field.child("sharedItems").is_none()
            && !matches!(field.attribute("databaseField"), Some("0" | "false"))
    };
    if definition
        .child("cacheSource")
        .and_then(|source| source.attribute("type"))
        != Some("worksheet")
        || !definition.child("cacheFields").is_some_and(|fields| {
            fields
                .children
                .iter()
                .any(|child| matches!(child, XmlContent::Element(field) if needs_items(field)))
        })
    {
        return std::borrow::Cow::Borrowed(definition);
    }
    let mut definition = definition.clone();
    if let Some(fields) = definition.child_mut("cacheFields") {
        for child in &mut fields.children {
            let XmlContent::Element(field) = child else {
                continue;
            };
            if !needs_items(field) {
                continue;
            }
            let name = field.name.rsplit_once(':').map_or_else(
                || "sharedItems".to_owned(),
                |(prefix, _)| format!("{prefix}:sharedItems"),
            );
            let mut items = XmlElement::new(name);
            for attribute in [
                "containsSemiMixedTypes",
                "containsNonDate",
                "containsString",
            ] {
                items.set_attribute(attribute, "0");
            }
            // Excel requires an empty dictionary even when records store literals directly.
            field.children.insert(0, XmlContent::Element(items));
        }
    }
    std::borrow::Cow::Owned(definition)
}

pub(crate) fn build_output(
    pivots: &[PivotTable],
    sheet_part: &str,
    numbers: &mut crate::features::package::PartNumbers,
    relationship_id: &mut impl FnMut(&str) -> String,
    output: &mut crate::features::package::SheetOutput,
) -> std::io::Result<()> {
    use crate::features::package::{
        OutputPart, OutputRelationship, PivotCacheAllocation, PivotCacheOutput, relative_target,
    };
    use std::io;
    for pivot in pivots {
        if pivot.definition.local_name() != "pivotTableDefinition"
            || pivot.definition.child("location").is_none()
            || pivot.definition.attribute("name").is_none_or(str::is_empty)
        {
            return Err(io::Error::other("Invalid pivot table definition"));
        }
        let key = Arc::as_ptr(&pivot.cache) as usize;
        if !numbers.pivot_caches.contains_key(&key) {
            let cache = &pivot.cache;
            cache
                .validate_worksheet_fields()
                .map_err(io::Error::other)?;
            if cache.definition.local_name() != "pivotCacheDefinition"
                || cache.definition.child("cacheSource").is_none()
                || cache.definition.child("cacheFields").is_none()
            {
                return Err(io::Error::other("Invalid pivot cache definition"));
            }
            let ids = cache
                .relationships
                .iter()
                .map(|rel| rel.id.as_ref())
                .collect();
            crate::drawing::chart_relationships(&cache.definition, &ids)
                .map_err(io::Error::other)?;
            if cache.records().is_some() {
                let mut records =
                    PivotRecords::new(cache, Default::default()).map_err(io::Error::other)?;
                let mut count = 0u64;
                while records.next_record().map_err(io::Error::other)?.is_some() {
                    count += 1;
                }
                if let Some(declared) = cache.definition.attribute("recordCount")
                    && declared.parse::<u64>().ok() != Some(count)
                {
                    return Err(io::Error::other(
                        "Pivot record count differs from cache definition",
                    ));
                }
            }
            let id = u32::try_from(numbers.pivot_cache_id)
                .map_err(|_| io::Error::other("Pivot cache ids exhausted"))?;
            numbers.pivot_cache_id += 1;
            let part = numbers.next_name(
                |n| &mut n.pivot_cache_file,
                ("xl/pivotCache/pivotCacheDefinition", ".xml"),
            );
            let mut bytes = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_vec();
            crate::drawing_xml::write(&mut bytes, &excel_cache_definition(&cache.definition), 0)?;
            output.parts.push(OutputPart {
                name: part.clone(),
                content_type: CT_CACHE.into(),
                bytes: bytes.into(),
            });
            crate::drawing::build_part_dependencies(
                &cache.relationships,
                &cache.parts,
                cache.source_part.as_deref(),
                &part,
                numbers,
                output,
            )?;
            output.pivot_caches.push(PivotCacheOutput {
                id,
                part: part.clone(),
                relationship: format!("rIdPivot{id}"),
            });
            numbers.pivot_caches.insert(
                key,
                PivotCacheAllocation {
                    cache: Arc::downgrade(cache),
                    id,
                    part,
                },
            );
        }
        let allocation = numbers
            .pivot_caches
            .get(&key)
            .ok_or_else(|| io::Error::other("Missing pivot cache allocation"))?;
        if allocation.cache.strong_count() == 0 {
            return Err(io::Error::other("Pivot cache allocation expired"));
        }
        let (id, cache_part) = (allocation.id, allocation.part.clone());
        let ids = pivot
            .relationships
            .iter()
            .map(|rel| rel.id.as_ref())
            .collect();
        crate::drawing::chart_relationships(&pivot.definition, &ids).map_err(io::Error::other)?;
        let part = numbers.next_name(|n| &mut n.pivot_file, ("xl/pivotTables/pivotTable", ".xml"));
        let mut definition = pivot.definition.clone();
        definition.set_attribute("cacheId", id.to_string());
        let mut bytes = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_vec();
        crate::drawing_xml::write(&mut bytes, &definition, 0)?;
        output.parts.push(OutputPart {
            name: part.clone(),
            content_type: CT_PIVOT.into(),
            bytes: bytes.into(),
        });
        let cache_source = pivot.cache.source_part.as_deref().unwrap_or("crabxl/cache");
        let edges: Vec<_> = pivot
            .relationships
            .iter()
            .filter(|rel| rel.kind.rsplit('/').next() == Some("pivotCacheDefinition"))
            .collect();
        if edges.len() != 1 || edges[0].external || edges[0].target.as_ref() != cache_source {
            return Err(io::Error::other("Invalid pivot table cache relationship"));
        }
        crate::drawing::build_part_dependencies_mapped(
            &pivot.relationships,
            &pivot.parts,
            pivot.source_part.as_deref(),
            &part,
            numbers,
            output,
            &[(cache_source, &cache_part)],
        )?;
        let id = relationship_id("pivot");
        output.refs.pivots.push(id.clone());
        output.relationships.push(OutputRelationship {
            id,
            kind: REL_PIVOT.into(),
            target: relative_target(sheet_part, &part),
        });
    }
    Ok(())
}
