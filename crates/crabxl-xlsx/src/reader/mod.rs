// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

mod budget;
mod cells;
mod metadata;
mod rows;
mod values;

use crate::xml::{Scope, XmlStream, append_xml_text};
use crabxl_core::{
    Cell, CellAddress, CellValue, Error, ErrorKind, ExactInteger, Formula, ReadOptions,
    ResourceLimits, Result, Row, RowBatch, RowIndex,
};
use quick_xml::events::{BytesStart, Event};
use std::{
    io::{BufReader, Read, Seek},
    iter::FusedIterator,
};
use zip::read::ZipFile;

/// A bounded, sparse worksheet row stream borrowing its workbook source.
///
/// Fully consuming the stream validates XML through EOF and the ZIP entry CRC.
/// Dropping early releases the borrow without reading or validating unread data.
/// An owned batch remains usable after this reader and its workbook are dropped.
pub struct Rows<'a, R: Read + Seek> {
    xml: XmlStream<BufReader<ZipFile<'a, R>>>,
    limits: ResourceLimits,
    options: ReadOptions,
    next_row_index: u32,
    capture_dimensions: bool,
    row_dimension: Option<crabxl_core::RowDimension>,
    capture_merges: bool,
    hyperlinks: Option<crate::hyperlinks::Capture>,
    merge_ranges: Vec<crabxl_core::CellRange>,
    last_row: Option<RowIndex>,
    exhausted: bool,
    pending: Option<Row>,
    value_buffer: String,
    decoded_cells: u64,
    projected_metadata_cells: u64,
    row_payload_bytes: usize,
    shared_strings: Option<&'a mut crate::strings::SharedStrings>,
    styles: Option<crate::style_reader::StyleRead<'a>>,
    epoch: crabxl_core::DateEpoch,
    shared_formulas: crate::formula_codec::SharedFormulas,
    aggregate: Option<crate::aggregate::ReadPool>,
    shared_string_policy: Option<&'a crate::SharedStringOptions>,
}
impl<'a, R: Read + Seek> Rows<'a, R> {
    pub(crate) fn new(
        input: BufReader<ZipFile<'a, R>>,
        part: String,
        limits: ResourceLimits,
        options: ReadOptions,
        shared_strings: Option<&'a mut crate::strings::SharedStrings>,
        styles: Option<crate::style_reader::StyleRead<'a>>,
        epoch: crabxl_core::DateEpoch,
    ) -> Result<Self> {
        if options.data_only
            && options.cell_metadata_policy
                == crabxl_core::CellMetadataReadPolicy::RetainFormulaReferences
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Formula annotation retention cannot be combined with data-only projection",
            )
            .with_part(part));
        }
        let mut xml = XmlStream::new(input, part, limits.max_part_bytes, limits);
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.depth == 1
                        && (frame.scope != Scope::Spreadsheet
                            || e.local_name().as_ref().as_bytes() != b"worksheet") =>
                {
                    return Err(
                        Error::new(ErrorKind::InvalidData, "Part is not a worksheet")
                            .with_part(xml.part()),
                    );
                }
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"sheetData" =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Worksheet has no sheetData element",
                    )
                    .with_part(xml.part()));
                }
                _ => {}
            }
        }
        Ok(Self {
            xml,
            limits,
            options,
            next_row_index: 0,
            capture_dimensions: false,
            capture_merges: false,
            hyperlinks: None,
            merge_ranges: Vec::new(),
            row_dimension: None,
            last_row: None,
            exhausted: false,
            pending: None,
            value_buffer: String::with_capacity(64.min(limits.max_cell_bytes)),
            decoded_cells: 0,
            projected_metadata_cells: 0,
            aggregate: None,
            shared_string_policy: None,
            row_payload_bytes: 0,
            shared_strings,
            styles,
            epoch,
            shared_formulas: crate::formula_codec::SharedFormulas::new(
                limits.max_formula_table_bytes,
                limits.max_shared_formulas,
            ),
        })
    }

    /// Count selected cells sent through scalar decoding, useful for projection diagnostics.
    pub fn decoded_cells(&self) -> u64 {
        self.decoded_cells
    }
    /// Successfully returned selected cells whose opaque cm/vm references were projected.
    /// Formula/visible value semantics are retained; metadata graphs are not interpreted.
    pub fn projected_metadata_cells(&self) -> u64 {
        self.projected_metadata_cells
    }

    /// Borrow the immutable prepared catalog while holding this row stream.
    /// Missing style parts retain the implicit General behavior of style zero.
    pub fn style_catalog(&self) -> Option<&crabxl_core::StyleCatalog> {
        self.styles.map(|styles| styles.catalog)
    }
    /// Worksheet-local template storage and expansion work, separate from projected cells.
    pub fn shared_formula_stats(&self) -> crate::SharedFormulaStats {
        self.shared_formulas.stats()
    }
    pub(crate) fn bytes_consumed(&self) -> u64 {
        self.xml.bytes_consumed()
    }

    pub(super) fn invalid(&self, message: &str) -> Error {
        Error::new(ErrorKind::InvalidData, message).with_part(self.xml.part())
    }
    pub(super) fn limit(&self, message: &str) -> Error {
        Error::new(ErrorKind::LimitExceeded, message).with_part(self.xml.part())
    }
}

impl<R: Read + Seek> Iterator for Rows<'_, R> {
    type Item = Result<Row>;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_row().transpose()
    }
}
impl<R: Read + Seek> FusedIterator for Rows<'_, R> {}

#[derive(Clone, Copy)]
enum ScalarKind {
    Numeric,
    Boolean,
    Text,
    InlineText,
    SharedText,
    IsoDate,
    Error,
}

enum BufferedValue {
    Direct(CellValue),
    SharedText(u64),
}

struct CellHeader {
    address: CellAddress,
    kind: ScalarKind,
    style: crabxl_core::StyleId,
    metadata: bool,
}
impl CellHeader {
    fn read(e: &BytesStart<'_>, row: RowIndex, column: u32) -> Result<Self> {
        Self::read_attributes(
            e.attributes().map(|attribute| {
                attribute.map_err(|error| {
                    Error::caused_by(ErrorKind::Xml, "Invalid cell attribute", error)
                })
            }),
            row,
            column,
        )
    }
    /// Read a plain `c r="A1" s="1" t="s"` tag, without its brackets, as the
    /// event path would. Anything else (other attributes or types, duplicates,
    /// entities, invalid values) returns None and retains the event path.
    fn buffered(tag: &[u8], row: RowIndex, column: u32) -> Option<Self> {
        let mut rest = tag.strip_prefix(b"c")?;
        let (mut address, mut kind, mut style) = (None, None, None);
        loop {
            let spaced = rest.first().is_some_and(u8::is_ascii_whitespace);
            rest = rest.trim_ascii_start();
            let [name, b'=', quote @ (b'"' | b'\''), tail @ ..] = rest else {
                if !rest.is_empty() {
                    return None;
                }
                return Some(Self {
                    address: match address {
                        Some(address) => address,
                        None => CellAddress::new(row.get(), column).ok()?,
                    },
                    kind: kind.unwrap_or(ScalarKind::Numeric),
                    style: style.unwrap_or(crabxl_core::StyleId::new(0)),
                    metadata: false,
                });
            };
            let end = tail.iter().position(|byte| byte == quote)?;
            let value = std::str::from_utf8(&tail[..end]).ok()?;
            if !spaced || value.contains(['&', '\t', '\n', '\r']) {
                return None;
            }
            match name {
                b'r' if address.is_none() => address = Some(value.parse().ok()?),
                b's' if style.is_none() => {
                    style = Some(crabxl_core::StyleId::new(value.parse().ok()?));
                }
                b't' if kind.is_none() => {
                    kind = Some(match value {
                        "n" => ScalarKind::Numeric,
                        "b" => ScalarKind::Boolean,
                        "s" => ScalarKind::SharedText,
                        _ => return None,
                    });
                }
                _ => return None,
            }
            rest = &tail[end + 1..];
        }
    }
    fn read_attributes<'a>(
        attributes: impl Iterator<Item = Result<quick_xml::events::attributes::Attribute<'a>>>,
        row: RowIndex,
        column: u32,
    ) -> Result<Self> {
        let mut address = None;
        let mut kind = ScalarKind::Numeric;
        let mut style = crabxl_core::StyleId::new(0);
        let mut metadata = false;
        for attribute in attributes {
            let attribute = attribute?;
            match attribute.key.as_ref().as_bytes() {
                b"r" => {
                    address = Some(
                        attribute
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|e| {
                                Error::caused_by(ErrorKind::Xml, "Invalid cell coordinate", e)
                            })?
                            .parse()?,
                    );
                }
                b"t" => {
                    let cell_type = attribute
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid cell type", e))?;
                    kind = match cell_type.as_ref() {
                        "n" => ScalarKind::Numeric,
                        "b" => ScalarKind::Boolean,
                        "str" => ScalarKind::Text,
                        "inlineStr" => ScalarKind::InlineText,
                        "s" => ScalarKind::SharedText,
                        "e" => ScalarKind::Error,
                        "d" => ScalarKind::IsoDate,
                        // Unknown types keep their text, as in openpyxl.
                        _ => ScalarKind::Text,
                    };
                }
                b"s" => {
                    style = crabxl_core::StyleId::new(
                        attribute
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|e| {
                                Error::caused_by(ErrorKind::Xml, "Invalid style index", e)
                            })?
                            .parse::<u32>()
                            .map_err(|e| {
                                Error::caused_by(ErrorKind::InvalidData, "Invalid style index", e)
                            })?,
                    );
                }
                b"cm" | b"vm" => {
                    metadata = true;
                }
                _ => {}
            }
        }
        Ok(Self {
            address: address.map_or_else(|| CellAddress::new(row.get(), column), Ok)?,
            kind,
            style,
            metadata,
        })
    }
}

fn numeric_value(value: &str) -> Result<CellValue> {
    if value.is_empty() {
        return Ok(CellValue::Empty);
    }
    if !value.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
        return Ok(match value.parse::<i64>() {
            Ok(integer) => CellValue::Integer(integer),
            Err(_) => CellValue::BigInteger(Box::new(ExactInteger::parse(value)?)),
        });
    }
    let number = fast_float2::parse::<f64, _>(value.as_bytes()).map_err(|error| {
        Error::caused_by(ErrorKind::InvalidData, "Invalid numeric cell value", error)
    })?;
    Ok(CellValue::Number(number))
}

fn shared_string_id(value: &str) -> Result<u64> {
    value.parse::<u64>().map_err(|error| {
        Error::caused_by(ErrorKind::InvalidData, "Invalid shared-string ID", error)
    })
}
fn boolean_value(value: &str) -> Result<CellValue> {
    if value.is_empty() {
        return Ok(CellValue::Empty);
    }
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Invalid boolean cell value",
        ));
    }
    Ok(CellValue::Boolean(digits.bytes().any(|byte| byte != b'0')))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(tag: &str) -> Option<(String, u32, &'static str)> {
        let header = CellHeader::buffered(tag.as_bytes(), RowIndex::new(1).ok()?, 2)?;
        let kind = match header.kind {
            ScalarKind::Numeric => "n",
            ScalarKind::Boolean => "b",
            ScalarKind::SharedText => "s",
            _ => "other",
        };
        Some((header.address.to_string(), header.style.get(), kind))
    }

    #[test]
    fn buffered_headers_match_plain_cells_only() {
        let some = |address: &str, style, kind| Some((address.to_owned(), style, kind));
        assert_eq!(header(r#"c r="B2""#), some("B2", 0, "n"));
        assert_eq!(header(r#"c r="AO2" s="3" t="s""#), some("AO2", 3, "s"));
        assert_eq!(header("c t='b'  r='D2'"), some("D2", 0, "b"));
        assert_eq!(header("c r=\"B2\"\tt=\"s\""), some("B2", 0, "s"));
        // Without a reference, the cell takes the next column of its row.
        assert_eq!(header("c"), some("C2", 0, "n"));
        // Everything else is left to the event reader.
        for tag in [
            r#"c r="B2" t="str""#,
            r#"c r="B2" cm="1""#,
            r#"c r="B2" r="C2""#,
            r#"c r="B2"s="1""#,
            r#"c r = "B2""#,
            r#"c r="B&#50;""#,
            "c r=\"B\t2\"",
            r#"c r="B2" s="x""#,
            r#"c r="2B""#,
            r#"c r="B2"#,
            r#"cell r="B2""#,
        ] {
            assert_eq!(header(tag), None, "{tag}");
        }
    }
}
