// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Cells operations.
use super::*;

impl<'a, R: Read + Seek> Rows<'a, R> {
    pub(super) fn read_row_impl(&mut self, row: &mut Row) -> Result<bool> {
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref().as_bytes() == b"row" =>
                {
                    let mut index = self.next_row_index;
                    let mut dimension_attributes = false;
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| {
                            Error::caused_by(ErrorKind::Xml, "Invalid row attribute", e)
                        })?;
                        if self.capture_dimensions
                            && !matches!(attr.key.as_ref().as_bytes(), b"r" | b"spans")
                        {
                            dimension_attributes = true;
                        }
                        if attr.key.as_ref().as_bytes() == b"r" {
                            let value = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
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
                    let index = RowIndex::new(index)?;
                    if self.capture_dimensions {
                        self.row_dimension = if dimension_attributes {
                            crate::dimension_codec::read_row(&e, index)?
                        } else {
                            None
                        };
                    }
                    if self.last_row.is_some_and(|last| index <= last) {
                        return Err(self.invalid("Rows must be in strictly increasing order"));
                    }
                    if self.options.stop_after_last_row
                        && self
                            .options
                            .rows
                            .as_ref()
                            .is_some_and(|range| index > *range.end())
                    {
                        self.exhausted = true;
                        return Ok(false);
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
                        && e.local_name().as_ref().as_bytes() == b"sheetData" =>
                {
                    self.finish_xml()?;
                    self.exhausted = true;
                    return Ok(false);
                }
                Event::Start(_) => return Err(self.invalid("Unexpected element in sheetData")),
                Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                    return Err(self.invalid("Unexpected text in sheetData"));
                }
                Event::Eof => return Err(self.invalid("Unexpected end of worksheet")),
                _ => {}
            }
        }
    }

    pub(super) fn read_cells(&mut self, row: &mut Row) -> Result<()> {
        let mut next_column = 0;
        loop {
            if let Some(cell) = self.buffered_scalar(row.index, next_column)? {
                next_column = cell.address.column.get() + 1;
                self.push_cell(row, cell)?;
                continue;
            }
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 4
                        && e.local_name().as_ref().as_bytes() == b"c" =>
                {
                    let header = CellHeader::read(&e, row.index, next_column)?;
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
                        None => Err(Error::new(
                            ErrorKind::InvalidData,
                            "Cell has a style ID without a style catalog",
                        )),
                    }
                    .map_err(|e| e.with_cell(header.address))?;
                    let annotations = if header.metadata
                        && self.options.cell_metadata_policy
                            == crabxl_core::CellMetadataReadPolicy::RetainFormulaReferences
                    {
                        let mut annotations = crabxl_core::FormulaAnnotations::default();
                        crate::metadata::attributes(&e, |name, value| {
                            if matches!(name, b"cm" | b"vm") {
                                if value.len()
                                    > self
                                        .limits
                                        .max_cell_bytes
                                        .saturating_sub(annotations.payload_bytes())
                                {
                                    return Err(Error::new(
                                        ErrorKind::LimitExceeded,
                                        "Formula annotation byte limit exceeded",
                                    ));
                                }
                                if name == b"cm" {
                                    annotations.cell_metadata = Some(value.into());
                                } else {
                                    annotations.value_metadata = Some(value.into());
                                }
                            }
                            Ok(())
                        })
                        .map_err(|error| {
                            error.with_part(self.xml.part()).with_cell(header.address)
                        })?;
                        Some(Box::new(annotations))
                    } else {
                        None
                    };
                    self.decoded_cells += 1;
                    let value = match header.kind {
                        ScalarKind::Boolean => self.read_cell::<true>(
                            header.address,
                            header.kind,
                            style_kind,
                            annotations,
                        ),
                        _ => self.read_cell::<false>(
                            header.address,
                            header.kind,
                            style_kind,
                            annotations,
                        ),
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
                    if header.metadata
                        && self.options.cell_metadata_policy
                            == crabxl_core::CellMetadataReadPolicy::Compatible
                    {
                        self.projected_metadata_cells += 1;
                    }
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"row" =>
                {
                    return Ok(());
                }
                Event::Start(_) => return Err(self.invalid("Unexpected element in row")),
                Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                    return Err(self.invalid("Unexpected text in row"));
                }
                Event::Eof => return Err(self.invalid("Unexpected end of row")),
                _ => {}
            }
        }
    }

    pub(super) fn buffered_scalar(&mut self, row: RowIndex, column: u32) -> Result<Option<Cell>> {
        let styles = self.styles;
        let options = &self.options;
        let decoded = &mut self.decoded_cells;
        let parsed = self
            .xml
            .buffered_scalar(|content, value| {
                let start = BytesStart::from_content(content, 1);
                let Some(header) = CellHeader::buffered(&start, row, column)? else {
                    return Ok(None);
                };
                if !matches!(
                    header.kind,
                    ScalarKind::Numeric | ScalarKind::Boolean | ScalarKind::SharedText
                ) || !options.includes(header.address)
                {
                    return Ok(None);
                }
                let address = header.address;
                if address.row != row || address.column.get() < column {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Cell coordinates do not follow their row order",
                    )
                    .with_cell(address));
                }
                let kind = match styles {
                    Some(styles) => styles.kind(header.style),
                    None if header.style.get() == 0 => Ok(None),
                    None => Err(Error::new(
                        ErrorKind::InvalidData,
                        "Cell has a style ID without a style catalog",
                    )),
                }
                .map_err(|error| error.with_cell(address))?;
                *decoded += 1;
                let value = match header.kind {
                    ScalarKind::Numeric => {
                        numeric_value(value.trim_ascii()).map(BufferedValue::Direct)
                    }
                    ScalarKind::Boolean => {
                        boolean_value(value.trim_ascii()).map(BufferedValue::Direct)
                    }
                    ScalarKind::SharedText => {
                        shared_string_id(value.trim_ascii()).map(BufferedValue::SharedText)
                    }
                    _ => return Ok(None),
                }
                .map_err(|error| error.with_cell(address))?;
                Ok(Some((header, value, kind)))
            })
            .map_err(|error| error.with_part(self.xml.part()))?;
        parsed
            .map(|(header, value, kind)| {
                let decoded = match value {
                    BufferedValue::Direct(value) => Ok(value),
                    BufferedValue::SharedText(id) => self.shared_string_value(id),
                }
                .map_err(|error| error.with_part(self.xml.part()).with_cell(header.address))?;
                let value = self
                    .interpret_date(decoded, kind)
                    .map_err(|error| error.with_part(self.xml.part()).with_cell(header.address))?;
                Ok(Cell {
                    address: header.address,
                    value,
                    style: header.style,
                })
            })
            .transpose()
    }

    pub(super) fn shared_string_value(&mut self, id: u64) -> Result<CellValue> {
        self.limit_string_cache()?;
        self.shared_strings
            .as_mut()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Cell refers to a missing shared-string table",
                )
            })?
            .get(id, self.options.rich_text)
    }

    pub(super) fn push_cell(&mut self, row: &mut Row, cell: Cell) -> Result<()> {
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

    pub(super) fn read_cell<const BOOLEAN: bool>(
        &mut self,
        address: CellAddress,
        kind: ScalarKind,
        date_kind: Option<crabxl_core::DateKind>,
        annotations: Option<Box<crabxl_core::FormulaAnnotations>>,
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
                        && e.local_name().as_ref().as_bytes() == b"v" =>
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
                        && e.local_name().as_ref().as_bytes() == b"is"
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
                        && e.local_name().as_ref().as_bytes() == b"f" =>
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
                        self.limits.max_cell_bytes,
                        self.options.formula_policy.into(),
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
                        && e.local_name().as_ref().as_bytes() == b"c" =>
                {
                    value = self.interpret_date(value, date_kind)?;
                    if let CellValue::RichText(rich) = &value
                        && rich
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
                    if seen_formula && self.options.data_only {
                        return Ok(value);
                    }
                    if let Some((expression, mut metadata)) = formula {
                        metadata.annotations = annotations;
                        if metadata.payload_bytes() > self.limits.max_cell_bytes {
                            return Err(self.limit("Formula metadata exceeds cell byte limit"));
                        }
                        if seen_value
                            && matches!(kind, ScalarKind::Text)
                            && matches!(value, CellValue::Empty)
                        {
                            value = CellValue::text("");
                        }
                        let preserve = metadata.annotations.is_some()
                            || self.options.formula_metadata
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
                    if annotations.is_some() {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Retaining scalar cm/vm metadata is not implemented",
                        ));
                    }
                    return Ok(value);
                }
                Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                    return Err(self.invalid("Unexpected text in cell"));
                }
                Event::Eof => return Err(self.invalid("Unexpected end of cell")),
                _ => {}
            }
        }
    }
}
