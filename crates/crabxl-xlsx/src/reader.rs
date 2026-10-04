// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

use crate::xml::{Scope, XmlStream};
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
    last_row: Option<RowIndex>,
    exhausted: bool,
    pending: Option<Row>,
    value_buffer: String,
    decoded_cells: u64,
    row_payload_bytes: usize,
}
impl<'a, R: Read + Seek> Rows<'a, R> {
    pub(crate) fn new(
        input: BufReader<ZipFile<'a, R>>,
        part: String,
        limits: ResourceLimits,
        options: ReadOptions,
    ) -> Result<Self> {
        let mut xml = XmlStream::new(input, part, limits.max_part_bytes, limits);
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.depth == 1
                        && (frame.scope != Scope::Spreadsheet
                            || e.local_name().as_ref() != b"worksheet") =>
                {
                    return Err(
                        Error::new(ErrorKind::InvalidData, "Part is not a worksheet")
                            .with_part(xml.part()),
                    );
                }
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref() == b"sheetData" =>
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
            last_row: None,
            exhausted: false,
            pending: None,
            value_buffer: String::with_capacity(64.min(limits.max_cell_bytes)),
            decoded_cells: 0,
            row_payload_bytes: 0,
        })
    }

    /// Count selected cells sent through scalar decoding, useful for projection diagnostics.
    pub fn decoded_cells(&self) -> u64 {
        self.decoded_cells
    }
    pub(crate) fn bytes_consumed(&self) -> u64 {
        self.xml.bytes_consumed()
    }

    /// Fill a caller-owned sparse row buffer, retaining its allocation for reuse.
    ///
    /// Returns false at EOF. On error, clears partial cells and terminates the
    /// stream; a caller may start another reader from the workbook afterwards.
    pub fn read_row_into(&mut self, row: &mut Row) -> Result<bool> {
        row.cells.clear();
        self.row_payload_bytes = 0;
        if let Some(pending) = self.pending.take() {
            *row = pending;
            return Ok(true);
        }
        if self.exhausted {
            return Ok(false);
        }
        let result = self
            .read_row_impl(row)
            .map_err(|error| error.with_part(self.xml.part()));
        if result.is_err() {
            self.exhausted = true;
            row.cells.clear();
        }
        result
    }

    /// Read the next owned row, or None after the stream is exhausted.
    pub fn next_row(&mut self) -> Result<Option<Row>> {
        let mut row = Row::new(RowIndex::new(0)?);
        self.read_row_into(&mut row)
            .map(|present| present.then_some(row))
    }

    /// Read an owned batch bounded by row count and estimated allocation bytes.
    ///
    /// At most one additional bounded row is retained as lookahead if it does
    /// not fit this batch. A single row larger than the batch budget is an error.
    pub fn read_batch(&mut self) -> Result<Option<RowBatch>> {
        let outer = self
            .limits
            .max_batch_bytes
            .checked_sub(size_of::<RowBatch>())
            .ok_or_else(|| self.limit("Batch byte limit is too small"))?;
        let capacity = self.limits.max_batch_rows.min(outer / size_of::<Row>());
        if capacity == 0 {
            return Err(self.limit("Batch byte limit is too small"));
        }
        let mut batch = RowBatch { rows: Vec::new() };
        batch.rows.try_reserve_exact(capacity).map_err(|e| {
            Error::caused_by(ErrorKind::LimitExceeded, "Cannot allocate batch", e)
                .with_part(self.xml.part())
        })?;
        let mut bytes = batch.memory_bytes();
        while batch.rows.len() < capacity {
            let Some(row) = self.next_row()? else {
                break;
            };
            let cell_bytes = row.memory_bytes() - size_of::<Row>();
            if bytes.saturating_add(cell_bytes) > self.limits.max_batch_bytes {
                if batch.rows.is_empty() {
                    self.exhausted = true;
                    return Err(self.limit("A row exceeds the batch byte limit"));
                }
                self.pending = Some(row);
                break;
            }
            bytes += cell_bytes;
            batch.rows.push(row);
        }
        Ok((!batch.rows.is_empty()).then_some(batch))
    }

    fn read_row_impl(&mut self, row: &mut Row) -> Result<bool> {
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref() == b"row" =>
                {
                    let mut index = self.next_row_index;
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| {
                            Error::caused_by(ErrorKind::Xml, "Invalid row attribute", e)
                        })?;
                        if attr.key.as_ref() == b"r" {
                            let value = attr
                                .decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    frame.decoder,
                                )
                                .map_err(|e| {
                                    Error::caused_by(ErrorKind::Xml, "Invalid row index", e)
                                })?;
                            index = value
                                .parse::<u32>()
                                .ok()
                                .and_then(|n| n.checked_sub(1))
                                .ok_or_else(|| {
                                    Error::new(ErrorKind::InvalidData, "Invalid row index")
                                })?;
                        }
                    }
                    let index = RowIndex::new(index).map_err(|e| e.with_part(self.xml.part()))?;
                    if self.last_row.is_some_and(|last| index <= last) {
                        return Err(self.invalid("Rows must be in strictly increasing order"));
                    }
                    row.index = index;
                    self.next_row_index = index.get() + 1;
                    self.last_row = Some(index);
                    self.read_cells(row)?;
                    if self.options.includes_row(index) {
                        return Ok(true);
                    }
                    row.cells.clear();
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 1
                        && e.local_name().as_ref() == b"sheetData" =>
                {
                    self.finish_xml()?;
                    self.exhausted = true;
                    return Ok(false);
                }
                Event::Start(_) => return Err(self.invalid("Unexpected element in sheetData")),
                Event::Text(t) if !t.iter().all(u8::is_ascii_whitespace) => {
                    return Err(self.invalid("Unexpected text in sheetData"));
                }
                Event::Eof => return Err(self.invalid("Unexpected end of worksheet")),
                _ => {}
            }
        }
    }

    fn read_cells(&mut self, row: &mut Row) -> Result<()> {
        let mut next_column = 0;
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 4
                        && e.local_name().as_ref() == b"c" =>
                {
                    let header = CellHeader::read(&e, frame.decoder, row.index, next_column)
                        .map_err(|e| e.with_part(self.xml.part()))?;
                    if header.address.row != row.index || header.address.column.get() < next_column
                    {
                        return Err(self
                            .invalid("Cell coordinates do not follow their row order")
                            .with_cell(header.address));
                    }
                    next_column = header.address.column.get() + 1;
                    if !self.options.includes(header.address) {
                        self.skip_cell()?;
                        continue;
                    }
                    if matches!(header.kind, ScalarKind::Unsupported)
                        || header.styled
                        || header.metadata
                    {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Selected cell type, style, or metadata is not supported yet",
                        )
                        .with_part(self.xml.part())
                        .with_cell(header.address));
                    }
                    self.decoded_cells += 1;
                    let value = match header.kind {
                        ScalarKind::Boolean => self.read_cell::<true>(header.kind),
                        _ => self.read_cell::<false>(header.kind),
                    }
                    .map_err(|e| e.with_part(self.xml.part()).with_cell(header.address))?;
                    self.push_cell(
                        row,
                        Cell {
                            address: header.address,
                            value,
                            style: crabxl_core::StyleId::new(0),
                        },
                    )?;
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref() == b"row" =>
                {
                    return Ok(());
                }
                Event::Start(_) => return Err(self.invalid("Unexpected element in row")),
                Event::Text(t) if !t.iter().all(u8::is_ascii_whitespace) => {
                    return Err(self.invalid("Unexpected text in row"));
                }
                Event::Eof => return Err(self.invalid("Unexpected end of row")),
                _ => {}
            }
        }
    }

    fn push_cell(&mut self, row: &mut Row, cell: Cell) -> Result<()> {
        if row.cells.len() >= self.limits.max_row_cells {
            return Err(self
                .limit("Row cell count limit exceeded")
                .with_cell(cell.address));
        }
        let payload = self
            .row_payload_bytes
            .saturating_add(cell.value.heap_bytes());
        if row.cells.len() == row.cells.capacity() {
            let max_capacity = self
                .limits
                .max_row_bytes
                .saturating_sub(size_of::<Row>())
                .saturating_sub(payload)
                / size_of::<Cell>();
            let wanted = (row.cells.capacity().saturating_mul(2))
                .max(16)
                .min(self.limits.max_row_cells)
                .min(max_capacity);
            if wanted <= row.cells.len() {
                return Err(self
                    .limit("Row byte limit exceeded")
                    .with_cell(cell.address));
            }
            row.cells
                .try_reserve_exact(wanted - row.cells.len())
                .map_err(|e| {
                    Error::caused_by(ErrorKind::LimitExceeded, "Cannot allocate row buffer", e)
                        .with_part(self.xml.part())
                })?;
        }
        if size_of::<Row>() + row.cells.capacity() * size_of::<Cell>() + payload
            > self.limits.max_row_bytes
        {
            return Err(self
                .limit("Row byte limit exceeded")
                .with_cell(cell.address));
        }
        self.row_payload_bytes = payload;
        row.cells.push(cell);
        Ok(())
    }

    fn read_cell<const BOOLEAN: bool>(&mut self, kind: ScalarKind) -> Result<CellValue> {
        let mut value = CellValue::Empty;
        let mut seen_value = false;
        let mut formula = None;
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 5
                        && e.local_name().as_ref() == b"v" =>
                {
                    if seen_value {
                        return Err(self.invalid("Cell has multiple value elements"));
                    }
                    if matches!(kind, ScalarKind::InlineText) {
                        return Err(self.invalid("Inline text cell contains a value element"));
                    }
                    seen_value = true;
                    value = self.read_value::<BOOLEAN>(kind)?;
                }
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 5
                        && e.local_name().as_ref() == b"is"
                        && matches!(kind, ScalarKind::InlineText) =>
                {
                    if seen_value {
                        return Err(self.invalid("Cell has multiple literal values"));
                    }
                    seen_value = true;
                    value = self.read_inline_text()?;
                }
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 5
                        && e.local_name().as_ref() == b"f" =>
                {
                    if formula.is_some() || matches!(kind, ScalarKind::InlineText) {
                        return Err(self.invalid("Invalid or duplicate formula element"));
                    }
                    for attribute in e.attributes() {
                        let attribute = attribute.map_err(|error| {
                            Error::caused_by(ErrorKind::Xml, "Invalid formula attribute", error)
                        })?;
                        if attribute.key.as_ref() != b"t" || attribute.value.as_ref() != b"normal" {
                            return Err(Error::new(
                                ErrorKind::Unsupported,
                                "Shared, array or other formula metadata is not supported yet",
                            ));
                        }
                    }
                    formula = Some(match self.read_value::<false>(ScalarKind::Text)? {
                        CellValue::Text(value) => value.as_str().to_owned(),
                        _ => return Err(self.invalid("Normal formula expression is empty")),
                    });
                }
                Event::Start(_) => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Other cell content is not supported yet",
                    )
                    .with_part(self.xml.part()));
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref() == b"c" =>
                {
                    if let Some(expression) = formula {
                        if seen_value
                            && matches!(kind, ScalarKind::Text)
                            && matches!(value, CellValue::Empty)
                        {
                            value = CellValue::text("");
                        }
                        if self.options.data_only {
                            return Ok(value);
                        }
                        return Ok(CellValue::Formula(Box::new(Formula::new(
                            expression.into_boxed_str(),
                            seen_value.then_some(value),
                        )?)));
                    }
                    return Ok(value);
                }
                Event::Text(t) if !t.iter().all(u8::is_ascii_whitespace) => {
                    return Err(self.invalid("Unexpected text in cell"));
                }
                Event::Eof => return Err(self.invalid("Unexpected end of cell")),
                _ => {}
            }
        }
    }

    fn read_value<const BOOLEAN: bool>(&mut self, kind: ScalarKind) -> Result<CellValue> {
        self.value_buffer.clear();
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Text(t) => {
                    let text = t.xml10_content().map_err(|e| {
                        Error::caused_by(ErrorKind::Xml, "Cannot decode scalar value", e)
                    })?;
                    append_value(&mut self.value_buffer, &text, self.limits.max_cell_bytes)?;
                }
                Event::CData(t) => {
                    let text = t.xml10_content().map_err(|e| {
                        Error::caused_by(ErrorKind::Xml, "Cannot decode scalar value", e)
                    })?;
                    append_value(&mut self.value_buffer, &text, self.limits.max_cell_bytes)?;
                }
                Event::GeneralRef(e) => {
                    let entity = e.decode().map_err(|e| {
                        Error::caused_by(ErrorKind::Xml, "Cannot decode scalar entity", e)
                    })?;
                    if let Some(text) = quick_xml::escape::resolve_xml_entity(&entity) {
                        append_value(&mut self.value_buffer, text, self.limits.max_cell_bytes)?;
                    } else if let Some(character) = e
                        .resolve_char_ref()
                        .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid scalar entity", e))?
                    {
                        let mut bytes = [0; 4];
                        append_value(
                            &mut self.value_buffer,
                            character.encode_utf8(&mut bytes),
                            self.limits.max_cell_bytes,
                        )?;
                    } else {
                        return Err(self.invalid("Unrecognized XML entity"));
                    }
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && (frame.depth == 4
                            || (matches!(kind, ScalarKind::InlineText) && frame.depth == 5))
                        && (e.local_name().as_ref() == b"v"
                            || e.local_name().as_ref() == b"t"
                            || e.local_name().as_ref() == b"f") =>
                {
                    break;
                }
                Event::Comment(_) | Event::PI(_) => {}
                _ => return Err(self.invalid("Invalid scalar value content")),
            }
        }
        if matches!(kind, ScalarKind::Text | ScalarKind::InlineText) {
            return Ok(
                if self.value_buffer.is_empty() && matches!(kind, ScalarKind::Text) {
                    CellValue::Empty
                } else {
                    CellValue::text(self.value_buffer.as_str())
                },
            );
        }
        if matches!(kind, ScalarKind::Error) {
            return Ok(if self.value_buffer.is_empty() {
                CellValue::Empty
            } else {
                CellValue::error(self.value_buffer.as_str())
            });
        }
        let value = self.value_buffer.trim_ascii();
        if value.is_empty() {
            return Ok(CellValue::Empty);
        }
        if BOOLEAN {
            let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(self.invalid("Invalid boolean cell value"));
            }
            return Ok(CellValue::Boolean(digits.bytes().any(|byte| byte != b'0')));
        }
        if !value.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
            return Ok(match value.parse::<i64>() {
                Ok(integer) => CellValue::Integer(integer),
                Err(_) => CellValue::BigInteger(Box::new(ExactInteger::parse(value)?)),
            });
        }
        let number = fast_float2::parse::<f64, _>(value.as_bytes()).map_err(|e| {
            Error::caused_by(ErrorKind::InvalidData, "Invalid numeric cell value", e)
                .with_part(self.xml.part())
        })?;
        if !number.is_finite() {
            return Err(self.invalid("Non-finite numeric value"));
        }
        Ok(CellValue::Number(number))
    }

    fn read_inline_text(&mut self) -> Result<CellValue> {
        let mut value = CellValue::text("");
        let mut seen = false;
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 6
                        && e.local_name().as_ref() == b"t" =>
                {
                    if seen {
                        return Err(self.invalid("Inline string has multiple plain text elements"));
                    }
                    seen = true;
                    value = self.read_value::<false>(ScalarKind::InlineText)?;
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 4
                        && e.local_name().as_ref() == b"is" =>
                {
                    return Ok(value);
                }
                Event::Text(t) if t.iter().all(u8::is_ascii_whitespace) => {}
                Event::Comment(_) | Event::PI(_) => {}
                Event::Start(_) => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Rich or phonetic inline text is not supported yet",
                    ));
                }
                _ => return Err(self.invalid("Invalid inline string content")),
            }
        }
    }

    fn skip_cell(&mut self) -> Result<()> {
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref() == b"c" =>
                {
                    return Ok(());
                }
                Event::Eof => return Err(self.invalid("Unexpected end of excluded cell")),
                _ => {}
            }
        }
    }
    fn finish_xml(&mut self) -> Result<()> {
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref() == b"sheetData" =>
                {
                    return Err(self.invalid("Duplicate sheetData element"));
                }
                Event::Eof => return Ok(()),
                _ => {}
            }
        }
    }
    fn invalid(&self, message: &str) -> Error {
        Error::new(ErrorKind::InvalidData, message).with_part(self.xml.part())
    }
    fn limit(&self, message: &str) -> Error {
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
    Error,
    Unsupported,
}

struct CellHeader {
    address: CellAddress,
    kind: ScalarKind,
    styled: bool,
    metadata: bool,
}
impl CellHeader {
    fn read(
        e: &BytesStart<'_>,
        decoder: quick_xml::encoding::Decoder,
        row: RowIndex,
        column: u32,
    ) -> Result<Self> {
        let mut address = None;
        let mut kind = ScalarKind::Numeric;
        let mut styled = false;
        let mut metadata = false;
        for attribute in e.attributes() {
            let attribute = attribute
                .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid cell attribute", e))?;
            match attribute.key.as_ref() {
                b"r" => {
                    address = Some(
                        attribute
                            .decoded_and_normalized_value(
                                quick_xml::XmlVersion::Implicit1_0,
                                decoder,
                            )
                            .map_err(|e| {
                                Error::caused_by(ErrorKind::Xml, "Invalid cell coordinate", e)
                            })?
                            .parse()?,
                    );
                }
                b"t" => {
                    let cell_type = attribute
                        .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
                        .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid cell type", e))?;
                    kind = match cell_type.as_ref() {
                        "n" => ScalarKind::Numeric,
                        "b" => ScalarKind::Boolean,
                        "str" => ScalarKind::Text,
                        "inlineStr" => ScalarKind::InlineText,
                        "e" => ScalarKind::Error,
                        _ => ScalarKind::Unsupported,
                    };
                }
                b"s" => {
                    styled = attribute
                        .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
                        .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid style index", e))?
                        .parse::<u32>()
                        .map_err(|e| {
                            Error::caused_by(ErrorKind::InvalidData, "Invalid style index", e)
                        })?
                        != 0;
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
            styled,
            metadata,
        })
    }
}
fn append_value(output: &mut String, text: &str, maximum: usize) -> Result<()> {
    if output.len().saturating_add(text.len()) > maximum {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            "Cell value byte limit exceeded",
        ));
    }
    output.try_reserve_exact(text.len()).map_err(|e| {
        Error::caused_by(
            ErrorKind::LimitExceeded,
            "Cannot allocate cell value buffer",
            e,
        )
    })?;
    output.push_str(text);
    Ok(())
}
