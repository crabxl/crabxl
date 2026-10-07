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
    Unsupported,
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
    fn buffered(e: &BytesStart<'_>, row: RowIndex, column: u32) -> Result<Option<Self>> {
        // Three distinct supported fields need no duplicate-check allocation.
        // Unknown, malformed and duplicate attributes retain the event path.
        let mut fields = [None, None, None];
        let mut seen = 0u8;
        for (count, attribute) in e.attributes().with_checks(false).enumerate() {
            let Ok(attribute) = attribute else {
                return Ok(None);
            };
            let bit = match attribute.key.as_ref().as_bytes() {
                b"r" => 1,
                b"s" => 2,
                b"t" => 4,
                _ => return Ok(None),
            };
            if seen & bit != 0 {
                return Ok(None);
            }
            seen |= bit;
            fields[count] = Some(attribute);
        }
        Self::read_attributes(fields.into_iter().flatten().map(Ok), row, column).map(Some)
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
                        _ => ScalarKind::Unsupported,
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
