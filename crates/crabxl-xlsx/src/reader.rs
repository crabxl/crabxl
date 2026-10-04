// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

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
    last_row: Option<RowIndex>,
    exhausted: bool,
    pending: Option<Row>,
    value_buffer: String,
    decoded_cells: u64,
    projected_metadata_cells: u64,
    row_payload_bytes: usize,
    shared_strings: Option<&'a mut crate::strings::SharedStrings>,
    styles: Option<&'a crate::style_reader::ImportedStyles>,
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
        styles: Option<&'a crate::style_reader::ImportedStyles>,
        epoch: crabxl_core::DateEpoch,
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

    pub(crate) fn with_read_pool(
        mut self,
        pool: Option<crate::aggregate::ReadPool>,
        string_policy: &'a crate::SharedStringOptions,
    ) -> Result<Self> {
        self.aggregate = pool;
        self.shared_string_policy = pool.map(|_| string_policy);
        self.limit_string_cache()?;
        Ok(self)
    }
    pub(crate) fn policy_catalog_bytes(&self) -> usize {
        self.aggregate
            .as_ref()
            .map_or(0, |pool| pool.fixed_bytes)
            .saturating_add(
                self.shared_strings
                    .as_ref()
                    .map_or(0, |strings| strings.minimum_managed_bytes()),
            )
    }
    pub(crate) fn shared_cache_bytes(&self) -> usize {
        self.shared_strings.as_ref().map_or(0, |strings| {
            strings
                .stats()
                .managed_bytes
                .saturating_sub(strings.minimum_managed_bytes())
        })
    }
    pub(crate) fn available_retained_bytes(&self) -> Result<usize> {
        self.aggregate.as_ref().map_or(Ok(usize::MAX), |pool| {
            let shared = self
                .shared_strings
                .as_ref()
                .map_or(0, |strings| strings.minimum_managed_bytes());
            pool.pool_bytes
                .checked_sub(shared)
                .and_then(|n| n.checked_sub(self.shared_formulas.stats().accounted_bytes))
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Aggregate component allowance exceeded",
                    )
                })
        })
    }
    pub(crate) fn set_aggregate_retained(&mut self, bytes: usize) -> Result<()> {
        if let Some(pool) = &mut self.aggregate {
            pool.retained_bytes = bytes;
        }
        self.limit_string_cache()
    }
    fn limit_string_cache(&mut self) -> Result<()> {
        if let Some(pool) = &self.aggregate {
            let maximum = pool.available(self.shared_formulas.stats().accounted_bytes)?;
            if let Some(strings) = &mut self.shared_strings {
                if let Some(policy) = self.shared_string_policy {
                    strings.limit_or_spill(policy, maximum)?;
                } else {
                    strings.limit_managed_bytes(maximum)?;
                }
            }
        }
        Ok(())
    }
    fn limit_formula_storage(
        &mut self,
        metadata: &crabxl_core::FormulaMetadata,
        expression: &str,
    ) -> Result<()> {
        if let (Some(pool), crabxl_core::FormulaType::Shared { index, .. }) =
            (&self.aggregate, metadata.kind)
        {
            let required = self.shared_formulas.required_bytes(index, expression.len());
            let available = pool.available(required)?;
            if let Some(strings) = &mut self.shared_strings {
                if let Some(policy) = self.shared_string_policy {
                    strings.limit_or_spill(policy, available)?;
                } else {
                    strings.limit_managed_bytes(available)?;
                }
            }
            let strings = self
                .shared_strings
                .as_ref()
                .map_or(0, |strings| strings.stats().managed_bytes);
            self.shared_formulas.set_maximum(
                pool.available(strings)?
                    .min(self.limits.max_formula_table_bytes),
            );
        }
        Ok(())
    }
    /// Live managed retained storage participating in this policy operation.
    /// Working reserve and caller-retained outputs are reported separately.
    pub fn managed_retained_bytes(&self) -> usize {
        let shared = self
            .shared_strings
            .as_ref()
            .map_or(0, |strings| strings.stats().managed_bytes);
        let formula = self.shared_formulas.stats().accounted_bytes;
        self.aggregate
            .as_ref()
            .map_or(shared.saturating_add(formula), |pool| {
                pool.fixed_bytes
                    .saturating_add(pool.retained_bytes)
                    .saturating_add(shared)
                    .saturating_add(formula)
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
        self.styles.map(|styles| &styles.catalog)
    }
    /// Worksheet-local template storage and expansion work, separate from projected cells.
    pub fn shared_formula_stats(&self) -> crate::SharedFormulaStats {
        self.shared_formulas.stats()
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
        let previous = self
            .aggregate
            .as_ref()
            .map_or(0, |pool| pool.retained_bytes);
        let result = self.read_batch_impl();
        if let Some(pool) = &mut self.aggregate {
            pool.retained_bytes = previous;
        }
        result
    }
    fn read_batch_impl(&mut self) -> Result<Option<RowBatch>> {
        let maximum = self
            .limits
            .max_batch_bytes
            .min(self.available_retained_bytes()?);
        let outer = maximum
            .checked_sub(size_of::<RowBatch>())
            .ok_or_else(|| self.limit("Batch byte limit is too small"))?;
        let row_slots = if self.aggregate.is_some() {
            size_of::<Row>().saturating_add(16 * size_of::<Cell>())
        } else {
            size_of::<Row>()
        };
        let capacity = self.limits.max_batch_rows.min(outer / row_slots);
        if capacity == 0 {
            return Err(self.limit("Batch byte limit is too small"));
        }
        self.set_aggregate_retained(
            size_of::<RowBatch>().saturating_add(capacity.saturating_mul(size_of::<Row>())),
        )?;
        let mut batch = RowBatch { rows: Vec::new() };
        batch.rows.try_reserve_exact(capacity).map_err(|e| {
            Error::caused_by(ErrorKind::LimitExceeded, "Cannot allocate batch", e)
                .with_part(self.xml.part())
        })?;
        let mut bytes = batch.memory_bytes();
        self.set_aggregate_retained(bytes)?;
        while batch.rows.len() < capacity {
            let Some(row) = self.next_row()? else {
                break;
            };
            let cell_bytes = row.memory_bytes() - size_of::<Row>();
            if bytes.saturating_add(cell_bytes) > maximum.min(self.available_retained_bytes()?) {
                if batch.rows.is_empty() {
                    self.exhausted = true;
                    return Err(self.limit("A row exceeds the batch byte limit"));
                }
                self.pending = Some(row);
                break;
            }
            bytes += cell_bytes;
            self.set_aggregate_retained(bytes)?;
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
                        self.skip_cell(header.address).map_err(|error| {
                            error.with_part(self.xml.part()).with_cell(header.address)
                        })?;
                        continue;
                    }
                    if matches!(header.kind, ScalarKind::Unsupported)
                        || (header.metadata
                            && self.options.cell_metadata_policy
                                == crabxl_core::CellMetadataReadPolicy::Reject)
                    {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Selected cell type, style, or metadata is not supported yet",
                        )
                        .with_part(self.xml.part())
                        .with_cell(header.address));
                    }
                    let style_kind = match self.styles {
                        Some(styles) => styles.kind(header.style),
                        None if header.style.get() == 0 => Ok(None),
                        None => Err(self.invalid("Cell has a style ID without a style catalog")),
                    }
                    .map_err(|e| e.with_cell(header.address))?;
                    self.decoded_cells += 1;
                    let value = match header.kind {
                        ScalarKind::Boolean => {
                            self.read_cell::<true>(header.address, header.kind, style_kind)
                        }
                        _ => self.read_cell::<false>(header.address, header.kind, style_kind),
                    }
                    .map_err(|e| e.with_part(self.xml.part()).with_cell(header.address))?;
                    self.push_cell(
                        row,
                        Cell {
                            address: header.address,
                            value,
                            style: header.style,
                        },
                    )?;
                    if header.metadata {
                        self.projected_metadata_cells += 1;
                    }
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

    fn read_cell<const BOOLEAN: bool>(
        &mut self,
        address: CellAddress,
        kind: ScalarKind,
        date_kind: Option<crabxl_core::DateKind>,
    ) -> Result<CellValue> {
        let mut value = CellValue::Empty;
        let mut seen_value = false;
        let mut formula = None;
        let mut seen_formula = false;
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
                    if seen_formula || matches!(kind, ScalarKind::InlineText) {
                        return Err(self.invalid("Invalid or duplicate formula element"));
                    }
                    seen_formula = true;
                    if self.options.data_only
                        && self.options.formula_policy == crabxl_core::FormulaReadPolicy::Compatible
                    {
                        self.skip_formula_text()?;
                        continue;
                    }
                    let mut metadata = crate::formula_codec::header(
                        &e,
                        frame.decoder,
                        self.limits.max_cell_bytes,
                    )?;
                    let mut expression = self.read_formula_text()?;
                    if !self.options.data_only
                        || self.options.formula_policy
                            == crabxl_core::FormulaReadPolicy::ValidateGroups
                    {
                        self.limit_formula_storage(&metadata, &expression)?;
                        expression = self.shared_formulas.resolve(
                            address,
                            expression,
                            &mut metadata,
                            self.options.formula_policy,
                            self.limits.max_cell_bytes,
                        )?;
                    }
                    formula = Some((expression, metadata));
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
                    value = self.interpret_date(value, date_kind)?;
                    if let CellValue::RichText(rich) = &value {
                        if rich
                            .phonetic_properties
                            .as_ref()
                            .is_some_and(|p| match self.styles {
                                Some(styles) => p.font_id as usize >= styles.catalog.fonts.len(),
                                None => p.font_id != 0,
                            })
                        {
                            return Err(
                                self.invalid("Phonetic text references a missing workbook font")
                            );
                        }
                    }
                    if seen_formula && self.options.data_only {
                        return Ok(value);
                    }
                    if let Some((expression, metadata)) = formula {
                        if seen_value
                            && matches!(kind, ScalarKind::Text)
                            && matches!(value, CellValue::Empty)
                        {
                            value = CellValue::text("");
                        }
                        let preserve = self.options.formula_metadata
                            || matches!(
                                metadata.kind,
                                crabxl_core::FormulaType::Array
                                    | crabxl_core::FormulaType::DataTable
                            )
                            || expression.is_empty();
                        return Ok(CellValue::Formula(Box::new(Formula::from_source(
                            expression,
                            seen_value.then_some(value),
                            preserve.then_some(metadata),
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
                event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)) => {
                    append_xml_text(&mut self.value_buffer, &event, self.limits.max_cell_bytes)?;
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
        if matches!(kind, ScalarKind::SharedText) {
            let id = self.value_buffer.trim_ascii().parse::<u64>().map_err(|e| {
                Error::caused_by(ErrorKind::InvalidData, "Invalid shared-string ID", e)
            })?;
            self.limit_string_cache()?;
            return self
                .shared_strings
                .as_mut()
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "Cell refers to a missing shared-string table",
                    )
                })?
                .get(id, self.options.rich_text);
        }
        if matches!(kind, ScalarKind::IsoDate) {
            return Ok(crabxl_core::parse_iso8601(&self.value_buffer)?
                .map_or(CellValue::Empty, |v| CellValue::DateTime(Box::new(v))));
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
        Ok(CellValue::Number(number))
    }

    fn interpret_date(
        &self,
        value: CellValue,
        kind: Option<crabxl_core::DateKind>,
    ) -> Result<CellValue> {
        use crabxl_core::{DateKind, DateReadPolicy, ExcelDateTime};
        let Some(mut kind) = kind else {
            return Ok(value);
        };
        let serial = match &value {
            CellValue::Integer(v) => *v as f64,
            CellValue::Number(v) => *v,
            CellValue::BigInteger(v) => v
                .as_str()
                .parse::<f64>()
                .map_err(|_| self.invalid("Invalid numeric date serial"))?,
            _ => return Ok(value),
        };
        if !serial.is_finite() {
            return if self.options.date_policy == DateReadPolicy::Compatible {
                Ok(CellValue::error("#VALUE!"))
            } else {
                Err(self.invalid("Non-finite date serial"))
            };
        }
        let raw = ExcelDateTime::from_serial(serial, self.epoch, kind)?;
        if kind == DateKind::DateTime
            && (0.0..1.0).contains(&serial)
            && raw.fraction_milliseconds() < 86_400_000
        {
            kind = DateKind::Time;
        }
        let date = ExcelDateTime::from_serial(serial, self.epoch, kind)?;
        if self.options.date_policy == DateReadPolicy::Compatible {
            let valid = date.is_reference_representable();
            if !valid {
                return Ok(CellValue::error("#VALUE!"));
            }
        }
        Ok(CellValue::DateTime(Box::new(date)))
    }
    fn read_inline_text(&mut self) -> Result<CellValue> {
        let mut parsed = crate::rich_text::read_container(
            &mut self.xml,
            5,
            b"is",
            self.limits.max_cell_bytes,
            self.options.rich_text,
        )?;
        if self.options.rich_text {
            parsed.unprotect();
        }
        Ok(parsed.into_value())
    }

    // Cache-only compatibility ignores expression semantics, but still validates XML.
    fn skip_formula_text(&mut self) -> Result<()> {
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Text(text) => {
                    text.xml10_content().map_err(|e| {
                        Error::caused_by(ErrorKind::Xml, "Cannot decode XML text", e)
                    })?;
                }
                Event::CData(text) => {
                    text.xml10_content().map_err(|e| {
                        Error::caused_by(ErrorKind::Xml, "Cannot decode XML text", e)
                    })?;
                }
                event @ Event::GeneralRef(_) => {
                    self.value_buffer.clear();
                    append_xml_text(&mut self.value_buffer, &event, 4)?;
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 4
                        && e.local_name().as_ref() == b"f" =>
                {
                    return Ok(());
                }
                Event::Comment(_) | Event::PI(_) => {}
                _ => return Err(self.invalid("Invalid formula text content")),
            }
        }
    }

    fn read_formula_text(&mut self) -> Result<Box<str>> {
        match self.read_value::<false>(ScalarKind::Text)? {
            CellValue::Text(value) => Ok((*value).into_string()),
            CellValue::Empty => Ok("".into()),
            _ => Err(self.invalid("Invalid formula text")),
        }
    }
    fn skip_cell(&mut self, address: CellAddress) -> Result<()> {
        let strict = self.options.formula_policy == crabxl_core::FormulaReadPolicy::ValidateGroups;
        let future_selected_rows = self
            .options
            .rows
            .as_ref()
            .is_none_or(|range| address.row <= *range.end());
        let retain_shared = strict || (!self.options.data_only && future_selected_rows);
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 5
                        && e.local_name().as_ref() == b"f"
                        && retain_shared =>
                {
                    if crate::formula_codec::is_shared(&e, frame.decoder)? {
                        let mut metadata = crate::formula_codec::header(
                            &e,
                            frame.decoder,
                            self.limits.max_cell_bytes,
                        )?;
                        let expression = self.read_formula_text()?;
                        let index = match metadata.kind {
                            crabxl_core::FormulaType::Shared { index, .. } => index,
                            _ => return Err(self.invalid("Invalid projected shared formula")),
                        };
                        if !self.shared_formulas.contains(index)
                            || self.options.formula_policy
                                == crabxl_core::FormulaReadPolicy::ValidateGroups
                        {
                            self.limit_formula_storage(&metadata, &expression)?;
                            self.shared_formulas.resolve(
                                address,
                                expression,
                                &mut metadata,
                                self.options.formula_policy,
                                self.limits.max_cell_bytes,
                            )?;
                        }
                    }
                }
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
    SharedText,
    IsoDate,
    Error,
    Unsupported,
}

struct CellHeader {
    address: CellAddress,
    kind: ScalarKind,
    style: crabxl_core::StyleId,
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
        let mut style = crabxl_core::StyleId::new(0);
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
                        "s" => ScalarKind::SharedText,
                        "e" => ScalarKind::Error,
                        "d" => ScalarKind::IsoDate,
                        _ => ScalarKind::Unsupported,
                    };
                }
                b"s" => {
                    style = crabxl_core::StyleId::new(
                        attribute
                            .decoded_and_normalized_value(
                                quick_xml::XmlVersion::Implicit1_0,
                                decoder,
                            )
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
