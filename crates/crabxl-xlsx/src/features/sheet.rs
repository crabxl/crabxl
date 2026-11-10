//! Worksheet-level feature dispatch: one pass reads every feature element of a
//! worksheet, writers emit them in `CT_Worksheet` order, and `Rewrite` replaces
//! them while an existing worksheet streams through the editor.
use super::*;
use crate::xml::{Scope, XmlStream, attribute};
use crabxl_core::{Result, SheetFeatures};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Relationship ids that a worksheet uses for feature parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PartRefs {
    /// `<tablePart r:id>` values in source order.
    pub tables: Vec<String>,
    /// Spreadsheet DrawingML relationship, preserved until explicitly replaced.
    pub drawing: Option<String>,
    pub pivots: Vec<String>,
    pub printer: Option<String>,
    /// `<legacyDrawing r:id>` (comment shapes).
    pub legacy_drawing: Option<String>,
}
impl PartRefs {
    pub(crate) fn is_empty(&self) -> bool {
        self.tables.is_empty()
            && self.legacy_drawing.is_none()
            && self.drawing.is_none()
            && self.pivots.is_empty()
            && self.printer.is_none()
    }
}

/// Worksheet elements that this module owns.
pub(crate) fn owns(name: &[u8]) -> bool {
    matches!(
        name,
        b"sheetProtection"
            | b"scenarios"
            | b"autoFilter"
            | b"sortState"
            | b"conditionalFormatting"
            | b"dataValidations"
            | b"headerFooter"
            | b"pivotTableParts"
            | b"drawing"
            | b"legacyDrawing"
            | b"tableParts"
    )
}

fn relationship_id(e: &BytesStart<'_>) -> Result<Option<String>> {
    for attr in e.attributes() {
        let attr = attr.map_err(|_| invalid("Invalid worksheet feature attribute"))?;
        let key: &str = attr.key.as_ref();
        if key.ends_with(":id") {
            return Ok(Some(
                attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(|_| invalid("Invalid relationship id"))?
                    .into_owned(),
            ));
        }
    }
    attribute(e, b"id")
}

/// Drop namespace prefixes and declarations; every scenario element is in the
/// worksheet namespace, which the writer declares on the root.
fn local_names(mut element: crabxl_core::XmlElement) -> crabxl_core::XmlElement {
    if let Some((_, local)) = element.name.split_once(':') {
        element.name = local.into();
    }
    element
        .attributes
        .retain(|(key, _)| key.as_ref() != "xmlns" && !key.starts_with("xmlns:"));
    for child in &mut element.children {
        if let crabxl_core::XmlContent::Element(child) = child {
            *child = local_names(std::mem::replace(child, crabxl_core::XmlElement::new("")));
        }
    }
    element
}

/// Read one owned element whose `Start` was just consumed at `depth` (2).
pub(crate) fn read_element<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
    features: &mut SheetFeatures,
    refs: &mut PartRefs,
) -> Result<()> {
    match start.local_name().as_ref().as_bytes() {
        b"sheetProtection" => {
            features.protection = Some(protection::read_sheet(xml, start, depth)?);
        }
        b"scenarios" => {
            features.scenarios = Some(local_names(crate::drawing_xml::read(xml, start, depth)?));
        }
        b"autoFilter" => {
            features.auto_filter = Some(auto_filter::read_auto_filter(xml, start, depth)?);
        }
        b"sortState" => {
            features.sort_state = Some(auto_filter::read_sort_state(xml, start, depth)?);
        }
        b"conditionalFormatting" => features
            .conditional_formats
            .push(conditional::read(xml, start, depth)?),
        b"dataValidations" => features.data_validations = validation::read(xml, start, depth)?,
        b"headerFooter" => {
            features.header_footer = Some(header_footer::read(xml, start, depth)?);
        }
        b"drawing" => {
            refs.drawing = Some(
                relationship_id(start)?.ok_or_else(|| invalid("Drawing has no relationship id"))?,
            );
            crate::style_codec::skip(xml, depth)?;
        }
        b"legacyDrawing" => {
            refs.legacy_drawing = relationship_id(start)?;
            crate::style_codec::skip(xml, depth)?;
        }
        b"tableParts" | b"pivotTableParts" => loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e) if frame.depth == depth + 1 => {
                    let pivot = start.local_name().as_ref() == "pivotTableParts";
                    if e.local_name().as_ref().as_bytes()
                        != if pivot {
                            b"pivotTablePart".as_slice()
                        } else {
                            b"tablePart".as_slice()
                        }
                    {
                        let depth = frame.depth;
                        crate::style_codec::skip(xml, depth)?;
                        continue;
                    }
                    let id = relationship_id(&e)?
                        .ok_or_else(|| invalid("tablePart has no relationship id"))?;
                    if pivot {
                        refs.pivots.push(id);
                    } else {
                        refs.tables.push(id);
                    }
                }
                Event::End(_) if frame.depth + 1 == depth => break,
                Event::Eof => return Err(invalid("Truncated tableParts")),
                _ => {}
            }
        },
        _ => crate::style_codec::skip(xml, depth)?,
    }
    Ok(())
}

/// Scan a whole worksheet part and collect its inline features and part refs.
/// Cells are skipped without decoding.
pub(crate) fn read_worksheet<B: BufRead>(
    xml: &mut XmlStream<B>,
) -> Result<(SheetFeatures, PartRefs)> {
    let mut features = SheetFeatures::default();
    let mut refs = PartRefs::default();
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 2 && frame.scope == Scope::Spreadsheet => {
                let name = e.local_name();
                let name = name.as_ref().as_bytes();
                if owns(name) {
                    let e = e.into_owned();
                    read_element(xml, &e, 2, &mut features, &mut refs)?;
                } else if name == b"pageSetup" {
                    refs.printer = frame.office_relationship.clone();
                } else if name == b"sheetData" {
                    crate::style_codec::skip(xml, 2)?;
                }
            }
            Event::Eof => return Ok((features, refs)),
            _ => {}
        }
    }
}

fn write_rid_element(
    out: &mut impl Write,
    name: &str,
    id: &str,
    uri: Option<&str>,
) -> io::Result<()> {
    write!(out, "<{name}")?;
    crate::printing::namespace(out, uri)?;
    write!(out, " xmlns:r=\"{R_NS}\" r:id=\"")?;
    out.write_all(quick_xml::escape::escape(id).as_bytes())?;
    out.write_all(b"\"/>")
}

/// Write the owned element at `rank` (a `WORKSHEET_ORDER` index), if present.
pub(crate) fn write_rank(
    out: &mut impl Write,
    features: &SheetFeatures,
    refs: &PartRefs,
    rank: usize,
    uri: Option<&str>,
) -> io::Result<()> {
    match WORKSHEET_ORDER.get(rank).copied() {
        Some(b"sheetProtection") => {
            if let Some(value) = &features.protection {
                protection::write_sheet(out, value, uri)?;
            }
        }
        Some(b"scenarios") => {
            if let Some(value) = &features.scenarios {
                let mut value = value.clone();
                if let Some(uri) = uri {
                    value.attributes.push(("xmlns".into(), uri.into()));
                }
                crate::drawing_xml::write(out, &value, 0)?;
            }
        }
        Some(b"autoFilter") => {
            if let Some(value) = &features.auto_filter {
                auto_filter::write_auto_filter(out, value, uri)?;
            }
        }
        Some(b"sortState") => {
            if let Some(value) = &features.sort_state {
                auto_filter::write_sort_state(out, value, uri)?;
            }
        }
        Some(b"conditionalFormatting") => {
            conditional::write(out, &features.conditional_formats, uri)?;
        }
        Some(b"dataValidations") => validation::write(out, &features.data_validations, uri)?,
        Some(b"headerFooter") => {
            if let Some(value) = &features.header_footer {
                header_footer::write(out, value, uri)?;
            }
        }
        Some(b"drawing") => {
            if let Some(id) = &refs.drawing {
                write_rid_element(out, "drawing", id, uri)?;
            }
        }
        Some(b"legacyDrawing") => {
            if let Some(id) = &refs.legacy_drawing {
                write_rid_element(out, "legacyDrawing", id, uri)?;
            }
        }
        Some(b"pivotTableParts") if !refs.pivots.is_empty() => {
            out.write_all(b"<pivotTableParts")?;
            crate::printing::namespace(out, uri)?;
            write!(out, " xmlns:r=\"{R_NS}\" count=\"{}\">", refs.pivots.len())?;
            for id in &refs.pivots {
                write!(
                    out,
                    "<pivotTablePart r:id=\"{}\"/>",
                    quick_xml::escape::escape(id)
                )?;
            }
            out.write_all(b"</pivotTableParts>")?;
        }
        Some(b"tableParts") if !refs.tables.is_empty() => {
            out.write_all(b"<tableParts")?;
            crate::printing::namespace(out, uri)?;
            write!(out, " xmlns:r=\"{R_NS}\" count=\"{}\">", refs.tables.len())?;
            for id in &refs.tables {
                out.write_all(b"<tablePart r:id=\"")?;
                out.write_all(quick_xml::escape::escape(id.as_str()).as_bytes())?;
                out.write_all(b"\"/>")?;
            }
            out.write_all(b"</tableParts>")?;
        }
        _ => {}
    }
    Ok(())
}

/// Write every owned element with a rank in `from..to`.
pub(crate) fn write_ranks(
    out: &mut impl Write,
    features: &SheetFeatures,
    refs: &PartRefs,
    from: usize,
    to: usize,
    uri: Option<&str>,
) -> io::Result<()> {
    for rank in from..to.min(WORKSHEET_ORDER.len()) {
        write_rank(out, features, refs, rank, uri)?;
    }
    Ok(())
}

pub(crate) fn rank(name: &[u8]) -> usize {
    worksheet_rank(name).unwrap_or(WORKSHEET_ORDER.len())
}

/// Streaming replacement of owned elements in an existing worksheet.
///
/// Owned source elements are skipped; replacements are written just before the
/// first source element whose rank is larger, or before `</worksheet>`. The
/// rewrite covers ranks in `from..to`, so that the editor can run one instance
/// before and one after the hyperlink and printing rewrites and keep schema
/// order between inserted elements.
pub(crate) struct Rewrite<'a> {
    features: &'a SheetFeatures,
    refs: &'a PartRefs,
    from: usize,
    next: usize,
    to: usize,
}
impl<'a> Rewrite<'a> {
    pub(crate) fn new(
        features: &'a SheetFeatures,
        refs: &'a PartRefs,
        from: usize,
        to: usize,
    ) -> Self {
        Self {
            features,
            refs,
            from,
            next: from,
            to,
        }
    }
    /// Call for every depth-2 SpreadsheetML `Start`. Returns true when the
    /// caller must skip the source element.
    pub(crate) fn before_start(
        &mut self,
        out: &mut impl Write,
        name: &[u8],
        uri: Option<&str>,
    ) -> io::Result<bool> {
        let rank = rank(name);
        if rank < self.next {
            // Already replaced (repeated element, or source out of schema order).
            return Ok(owns(name) && rank >= self.from && rank < self.to);
        }
        let stop = rank.min(self.to);
        write_ranks(out, self.features, self.refs, self.next, stop, uri)?;
        self.next = self.next.max(stop);
        if owns(name) && rank < self.to {
            write_rank(out, self.features, self.refs, rank, uri)?;
            self.next = rank + 1;
            return Ok(true);
        }
        Ok(false)
    }
    /// Call before `</worksheet>`.
    pub(crate) fn finish(&mut self, out: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        write_ranks(out, self.features, self.refs, self.next, self.to, uri)?;
        self.next = self.to;
        Ok(())
    }
}

/// Rank boundary between the early rewrite (before hyperlinks) and the late one.
pub(crate) fn hyperlinks_rank() -> usize {
    rank(b"hyperlinks")
}
/// First rank after the printing elements (`colBreaks`).
pub(crate) fn after_printing_rank() -> usize {
    rank(b"colBreaks") + 1
}
