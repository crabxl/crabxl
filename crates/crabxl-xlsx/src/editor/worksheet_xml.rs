//! Worksheet overlays and canonical model XML rewriting.
use super::*;

pub(super) fn unsigned_attribute(e: &BytesStart<'_>, name: &[u8]) -> Result<Option<u32>> {
    let Some(attribute) = e
        .try_get_attribute(
            std::str::from_utf8(name)
                .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid attribute name", e))?,
        )
        .map_err(|error| Error::caused_by(ErrorKind::Xml, "Invalid position attribute", error))?
    else {
        return Ok(None);
    };
    let value = attribute
        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
        .map_err(|error| Error::caused_by(ErrorKind::Xml, "Invalid position value", error))?;
    value.parse().map(Some).map_err(|error| {
        Error::caused_by(ErrorKind::InvalidData, "Invalid position integer", error)
    })
}
pub(super) fn patched_start(
    e: &BytesStart<'_>,
    uri: &str,
    value: &CellValue,
) -> Result<BytesStart<'static>> {
    let mut start = e.to_owned();
    start.clear_attributes();
    for attribute in e.attributes() {
        let attribute = attribute
            .map_err(|error| Error::caused_by(ErrorKind::Xml, "Invalid cell attribute", error))?;
        if !matches!(
            attribute.key.as_ref().as_bytes(),
            b"t" | b"xmlns" | b"cm" | b"vm"
        ) {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("xmlns", uri));
    let literal = match value {
        CellValue::Formula(formula) => formula.cached(),
        value => Some(value),
    };
    match literal {
        Some(CellValue::Text(_) | CellValue::RichText(_)) => start.push_attribute((
            "t",
            if matches!(value, CellValue::Formula(_)) {
                "str"
            } else {
                "inlineStr"
            },
        )),
        Some(CellValue::Boolean(_)) => start.push_attribute(("t", "b")),
        Some(CellValue::Error(_)) => start.push_attribute(("t", "e")),
        _ => {}
    }
    Ok(start)
}
pub(super) fn write_body<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    cell: &Cell,
    buffer: &mut RowBuffer,
    limits: ResourceLimits,
    formula_attributes: crate::FormulaWritePolicy,
) -> Result<()> {
    encode_cells(
        buffer,
        cell.address.row,
        std::slice::from_ref(cell).iter(),
        limits.max_cell_bytes,
        1,
        StyleContext::Appearance(&[CellStyle::default()]),
        ValueEncoding {
            epoch: DateEpoch::Windows1900,
            iso_dates: false,
            non_finite: crate::NonFiniteWritePolicy::Blank,
            formula_attributes,
            invalidate_caches: false,
            date_styles: crate::encode::DateStyleIds {
                datetime: crabxl_core::StyleId::new(1),
                time: crabxl_core::StyleId::new(2),
                duration: crabxl_core::StyleId::new(3),
                date: crabxl_core::StyleId::new(4),
            },
        },
    )
    .map_err(|error| error.with_cell(cell.address))?;
    // Reuse the shared cell body under a namespace-aware original/new header.
    let begin = buffer
        .data
        .iter()
        .position(|byte| *byte == b'>')
        .and_then(|row_end| {
            buffer.data[row_end + 1..]
                .iter()
                .position(|byte| *byte == b'>')
                .map(|cell_end| row_end + cell_end + 2)
        })
        .ok_or_else(|| invalid("Encoded cell header is missing"))?;
    let end = buffer
        .data
        .len()
        .checked_sub(b"</c></row>".len())
        .ok_or_else(|| invalid("Encoded cell footer is missing"))?;
    writer
        .get_mut()
        .write_all(&buffer.data[begin..end])
        .map_err(|error| {
            Error::caused_by(
                if error.kind() == io::ErrorKind::FileTooLarge {
                    ErrorKind::LimitExceeded
                } else {
                    ErrorKind::Io
                },
                "Cannot write replacement cell body",
                error,
            )
        })
}
pub(super) fn write_inserted_cell<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    patch: &Patch,
    uri: &str,
    buffer: &mut RowBuffer,
    limits: ResourceLimits,
    formula_attributes: crate::FormulaWritePolicy,
) -> Result<()> {
    if !patch.insert_missing {
        return Err(
            invalid("Pending replacement targets a missing physical cell")
                .with_cell(patch.cell.address),
        );
    }
    let mut base = BytesStart::new("c");
    let reference = patch.cell.address.to_string();
    base.push_attribute(("r", reference.as_str()));
    let start = patched_start(&base, uri, &patch.cell.value)?;
    emit(writer, Event::Start(start))?;
    write_body(writer, &patch.cell, buffer, limits, formula_attributes)?;
    emit(writer, Event::End(quick_xml::events::BytesEnd::new("c")))
}
pub(super) fn positioned_start(
    e: &BytesStart<'_>,
    position: &str,
    omit_spans: bool,
) -> Result<BytesStart<'static>> {
    let mut start = e.to_owned();
    start.clear_attributes();
    for attribute in e.attributes() {
        let attribute = attribute.map_err(|error| {
            Error::caused_by(ErrorKind::Xml, "Invalid coordinate attribute", error)
        })?;
        if attribute.key.as_ref().as_bytes() != b"r"
            && !(omit_spans && attribute.key.as_ref().as_bytes() == b"spans")
        {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("r", position));
    Ok(start)
}
pub(super) fn expanded_dimension(
    e: &BytesStart<'_>,
    patches: &Patches,
) -> Result<BytesStart<'static>> {
    let reference =
        attribute(e, b"ref")?.ok_or_else(|| invalid("Worksheet dimension has no reference"))?;
    let (first, last) = reference
        .split_once(':')
        .unwrap_or((&reference, &reference));
    let first: CellAddress = first.parse()?;
    let last: CellAddress = last.parse()?;
    let mut low = (first.row.get(), first.column.get());
    let mut high = (last.row.get(), last.column.get());
    if low.0 > high.0 || low.1 > high.1 {
        return Err(invalid("Worksheet dimension is reversed"));
    }
    for patch in patches.values().filter(|patch| patch.insert_missing) {
        let address = patch.cell.address;
        low.0 = low.0.min(address.row.get());
        low.1 = low.1.min(address.column.get());
        high.0 = high.0.max(address.row.get());
        high.1 = high.1.max(address.column.get());
    }
    let reference = format!(
        "{}:{}",
        CellAddress::new(low.0, low.1)?,
        CellAddress::new(high.0, high.1)?
    );
    let mut start = e.to_owned();
    start.clear_attributes();
    for attribute in e.attributes() {
        let attribute = attribute.map_err(|error| {
            Error::caused_by(ErrorKind::Xml, "Invalid dimension attribute", error)
        })?;
        if attribute.key.as_ref().as_bytes() != b"ref" {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("ref", reference.as_str()));
    Ok(start)
}
pub(super) struct WorksheetRewrite<'a> {
    pub(super) patches: Option<&'a Patches>,
    pub(super) limits: ResourceLimits,
    pub(super) formula_attributes: crate::FormulaWritePolicy,
    pub(super) views: Option<&'a crabxl_core::SheetViews>,
    pub(super) printing: Option<&'a crabxl_core::PrintSettings>,
    pub(super) hyperlinks: Option<(
        &'a crabxl_core::Hyperlinks,
        &'a crate::hyperlinks::source::Plan,
    )>,
    pub(super) invalidate_caches: bool,
    pub(super) model: Option<&'a crabxl_core::Worksheet>,
    pub(super) catalog: Option<&'a crabxl_core::StyleCatalog>,
    pub(super) epoch: DateEpoch,
    pub(super) non_finite: crate::NonFiniteWritePolicy,
    pub(super) copying: bool,
}
pub(super) fn patch_worksheet<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    rewrite: WorksheetRewrite<'_>,
) -> Result<u64> {
    let WorksheetRewrite {
        patches,
        limits,
        formula_attributes,
        views,
        printing,
        hyperlinks,
        invalidate_caches,
        model,
        catalog,
        epoch,
        non_finite,
        copying,
    } = rewrite;
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let mut print_rewrite = printing.map(crate::printing::Rewrite::new);
    let mut row = 0u32;
    let mut next_row = 0u32;
    let mut next_column = 0u32;
    let mut last_row = None;
    let mut selected_row = false;
    let patches = patches.filter(|_| model.is_none());
    let mut pending = patches
        .into_iter()
        .flat_map(|patches| patches.values())
        .peekable();
    let mut data_uri = None;
    let mut row_tail = false;
    let mut seen_data = false;
    let mut found = 0usize;
    let mut in_data = false;
    let mut in_row = false;
    let mut in_cell = false;
    let mut views_written = false;
    let mut hyperlink_rewrite =
        hyperlinks.map(|(links, plan)| crate::hyperlinks::rewrite::Rewrite::new(links, plan));
    let mut skipped_views = false;
    let mut formula = false;
    let mut seen_v = false;
    let mut buffer = RowBuffer {
        data: Vec::new(),
        maximum: limits.max_row_bytes,
    };
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if let Some(rewrite) = &mut hyperlink_rewrite
            && frame.scope == Scope::Spreadsheet
        {
            match &frame.event {
                Event::Start(e) => {
                    let skip = rewrite
                        .before_start(
                            writer.get_mut(),
                            e.local_name().as_ref().as_bytes(),
                            frame.depth,
                            frame.spreadsheet_uri,
                        )
                        .map_err(|cause| io_error("Cannot replace source hyperlinks", cause))?;
                    if skip {
                        let depth = frame.depth;
                        crate::style_codec::skip(&mut xml, depth)?;
                        continue;
                    }
                }
                Event::End(_) => rewrite
                    .before_end(writer.get_mut(), frame.depth, frame.spreadsheet_uri)
                    .map_err(|cause| io_error("Cannot finish source hyperlinks", cause))?,
                _ => {}
            }
        }

        if matches!(&frame.event,Event::Start(e) if e.local_name().as_ref().as_bytes()==b"AlternateContent")
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing markup-compatibility alternatives requires typed branch handling",
            ));
        }
        // openpyxl's public copy behavior omits views and header/footer state.
        if copying
            && frame.scope == Scope::Spreadsheet
            && frame.depth == 2
            && matches!(&frame.event, Event::Start(e)
                if matches!(e.local_name().as_ref().as_bytes(), b"sheetViews" | b"headerFooter"))
        {
            let depth = frame.depth;
            crate::style_codec::skip(&mut xml, depth)?;
            continue;
        }
        if let Some(rewrite) = &mut print_rewrite
            && frame.scope == Scope::Spreadsheet
        {
            match &frame.event {
                Event::Start(e) => {
                    let skip = rewrite
                        .before_start(
                            writer.get_mut(),
                            e.local_name().as_ref().as_bytes(),
                            frame.depth,
                            frame.spreadsheet_uri,
                        )
                        .map_err(|error| io_error("Cannot replace printing metadata", error))?;
                    if skip {
                        let depth = frame.depth;
                        crate::style_codec::skip(&mut xml, depth)?;
                        continue;
                    }
                }
                Event::End(e) => rewrite
                    .before_end(
                        writer.get_mut(),
                        e.local_name().as_ref().as_bytes(),
                        frame.depth,
                        frame.spreadsheet_uri,
                    )
                    .map_err(|error| io_error("Cannot finish printing metadata", error))?,
                _ => {}
            }
        }
        if let Some(views) = views
            && let Event::Start(e) = &frame.event
            && frame.depth == 2
            && frame.scope == Scope::Spreadsheet
        {
            let name = e.local_name();
            if !views_written && !matches!(name.as_ref().as_bytes(), b"sheetPr" | b"dimension") {
                crate::worksheet_view::write_views(writer.get_mut(), views, frame.spreadsheet_uri)
                    .map_err(|error| io_error("Cannot write worksheet views", error))?;
                views_written = true;
            }
            if name.as_ref().as_bytes() == b"sheetViews" {
                if skipped_views {
                    return Err(invalid("Duplicate worksheet views container"));
                }
                skipped_views = true;
                loop {
                    let old = xml.next()?;
                    if matches!(&old.event, Event::End(e) if old.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheetViews")
                    {
                        break;
                    }
                    if matches!(old.event, Event::Eof) {
                        return Err(invalid("Incomplete replaced worksheet views"));
                    }
                }
                continue;
            }
        }
        if let Some(model) = model
            && let Event::Start(e) = &frame.event
            && frame.scope == Scope::Spreadsheet
            && frame.depth == 2
        {
            match e.local_name().as_ref().as_bytes() {
                b"dimension" => {
                    let mut start = e.to_owned();
                    start.clear_attributes();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|cause| {
                            Error::caused_by(ErrorKind::Xml, "Invalid dimension attribute", cause)
                        })?;
                        if attr.key.as_ref().as_bytes() != b"ref" {
                            start.push_attribute(attr);
                        }
                    }
                    let reference = crate::loaded_codec::dimension(model);
                    start.push_attribute(("ref", reference.as_str()));
                    emit(&mut writer, Event::Start(start))?;
                    continue;
                }
                b"cols" | b"mergeCells" => {
                    let depth = frame.depth;
                    crate::style_codec::skip(&mut xml, depth)?;
                    continue;
                }
                b"sheetData" => {
                    if seen_data {
                        return Err(invalid("Duplicate sheetData in affected worksheet"));
                    }
                    seen_data = true;
                    crate::dimension_codec::write_columns(
                        writer.get_mut(),
                        model.dimensions().columns(),
                        frame.spreadsheet_uri,
                    )
                    .map_err(|error| io_error("Cannot write model columns", error))?;
                    crate::loaded_codec::write_data(
                        writer.get_mut(),
                        model,
                        catalog,
                        limits,
                        crate::loaded_codec::Encoding {
                            epoch,
                            non_finite,
                            formula_attributes,
                        },
                        frame
                            .spreadsheet_uri
                            .ok_or_else(|| invalid("Missing worksheet namespace"))?,
                    )?;
                    crate::loaded_codec::write_merges(
                        writer.get_mut(),
                        model,
                        frame.spreadsheet_uri,
                    )?;
                    let depth = frame.depth;
                    crate::style_codec::skip(&mut xml, depth)?;
                    continue;
                }
                _ => {}
            }
        }
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"worksheet"
                {
                    return Err(invalid("Affected part is not a worksheet"));
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"dimension"
                    && patches.is_some() =>
            {
                let start =
                    expanded_dimension(&e, patches.ok_or_else(|| invalid("Missing overlays"))?)?;
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"mergeCell"
                    && patches.is_some() =>
            {
                let reference = attribute(&e, b"ref")?
                    .ok_or_else(|| invalid("Merged range has no reference"))?;
                let (first, last) = reference
                    .split_once(':')
                    .unwrap_or((&reference, &reference));
                let start: CellAddress = first.parse()?;
                let end: CellAddress = last.parse()?;
                let range = crabxl_core::CellRange::new(start, end)?;
                for patch in patches.into_iter().flat_map(|patches| {
                    patches
                        .range((start.row.get(), 0)..=(end.row.get(), u32::MAX))
                        .map(|(_, patch)| patch)
                }) {
                    if range.contains(patch.cell.address) && patch.cell.address != start {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Editing a non-anchor merged cell is not supported",
                        )
                        .with_cell(patch.cell.address));
                    }
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheetData" =>
            {
                if seen_data {
                    return Err(invalid("Duplicate sheetData in affected worksheet"));
                }
                seen_data = true;
                in_data = true;
                data_uri = frame.spreadsheet_uri;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if in_data
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"row" =>
            {
                in_row = true;
                row = unsigned_attribute(&e, b"r")?
                    .map(|value| {
                        value
                            .checked_sub(1)
                            .ok_or_else(|| invalid("Invalid row index"))
                    })
                    .transpose()?
                    .unwrap_or(next_row);
                crabxl_core::RowIndex::new(row)?;
                while pending
                    .peek()
                    .is_some_and(|patch| patch.cell.address.row.get() < row)
                {
                    let inserted_row = pending
                        .peek()
                        .ok_or_else(|| invalid("Missing pending row"))?
                        .cell
                        .address
                        .row
                        .get();
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    let mut start = BytesStart::new("row");
                    let reference = (inserted_row + 1).to_string();
                    start.push_attribute(("r", reference.as_str()));
                    start.push_attribute(("xmlns", uri));
                    emit(&mut writer, Event::Start(start))?;
                    while pending
                        .peek()
                        .is_some_and(|patch| patch.cell.address.row.get() == inserted_row)
                    {
                        let patch = pending
                            .next()
                            .ok_or_else(|| invalid("Missing pending cell"))?;
                        write_inserted_cell(
                            &mut writer,
                            patch,
                            uri,
                            &mut buffer,
                            limits,
                            formula_attributes,
                        )?;
                        found += 1;
                    }
                    emit(
                        &mut writer,
                        Event::End(quick_xml::events::BytesEnd::new("row")),
                    )?;
                }
                row_tail = false;
                if last_row.is_some_and(|last| row <= last) {
                    return Err(invalid("Affected worksheet rows are not ordered"));
                }
                last_row = Some(row);
                next_row = row + 1;
                next_column = 0;
                selected_row = patches.is_some_and(|patches| {
                    patches.range((row, 0)..=(row, u32::MAX)).next().is_some()
                });
                let start = positioned_start(&e, &(row + 1).to_string(), selected_row)?;
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Start(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 4
                    && e.local_name().as_ref().as_bytes() == b"extLst" =>
            {
                row_tail = true;
                while pending
                    .peek()
                    .is_some_and(|patch| patch.cell.address.row.get() == row)
                {
                    let patch = pending
                        .next()
                        .ok_or_else(|| invalid("Missing pending cell"))?;
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    write_inserted_cell(
                        &mut writer,
                        patch,
                        uri,
                        &mut buffer,
                        limits,
                        formula_attributes,
                    )?;
                    found += 1;
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 4
                    && e.local_name().as_ref().as_bytes() == b"c" =>
            {
                if row_tail {
                    return Err(invalid("Cell follows row extension list"));
                }
                in_cell = true;
                formula = false;
                seen_v = false;
                let replacement = if selected_row {
                    let address = attribute(&e, b"r")?
                        .map(|value| value.parse::<CellAddress>())
                        .transpose()?
                        .unwrap_or(CellAddress::new(row, next_column)?);
                    if address.row.get() != row || address.column.get() < next_column {
                        return Err(
                            invalid("Affected cell coordinates are not ordered").with_cell(address)
                        );
                    }
                    while pending.peek().is_some_and(|patch| {
                        patch.cell.address.row.get() == row
                            && patch.cell.address.column.get() < address.column.get()
                    }) {
                        let patch = pending
                            .next()
                            .ok_or_else(|| invalid("Missing pending cell"))?;
                        let uri =
                            data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                        write_inserted_cell(
                            &mut writer,
                            patch,
                            uri,
                            &mut buffer,
                            limits,
                            formula_attributes,
                        )?;
                        found += 1;
                    }
                    next_column = address.column.get() + 1;
                    if pending
                        .peek()
                        .is_some_and(|patch| patch.cell.address == address)
                    {
                        pending.next().map(|patch| &patch.cell)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(cell) = replacement {
                    let uri = frame
                        .spreadsheet_uri
                        .ok_or_else(|| invalid("Affected cell namespace is missing"))?;
                    let positioned = positioned_start(&e, &cell.address.to_string(), false)?;
                    let start = patched_start(&positioned, uri, &cell.value)
                        .map_err(|error| error.with_cell(cell.address))?;
                    let name = start.name().as_ref().as_bytes().to_vec();
                    // Validate the old cell before replacing its body.
                    loop {
                        let old = xml.next()?;
                        match &old.event {
                            Event::End(end)
                                if old.depth == 3
                                    && end.local_name().as_ref().as_bytes() == b"c" =>
                            {
                                break;
                            }
                            Event::Start(child)
                                if old.scope == Scope::Spreadsheet
                                    && old.depth == 5
                                    && child.local_name().as_ref().as_bytes() == b"is" =>
                            {
                                crate::rich_text::read_container(
                                    &mut xml,
                                    5,
                                    b"is",
                                    limits.max_cell_bytes,
                                    true,
                                )
                                .map_err(|e| e.with_cell(cell.address))?;
                            }
                            Event::Start(child) => {
                                if old.scope != Scope::Spreadsheet
                                    || !matches!(
                                        child.local_name().as_ref().as_bytes(),
                                        b"v" | b"is" | b"t" | b"f"
                                    )
                                {
                                    return Err(Error::new(ErrorKind::Unsupported,"Replacing unknown or rich cell content requires typed support").with_cell(cell.address));
                                }
                                if child.local_name().as_ref().as_bytes() == b"f" {
                                    let metadata = crate::formula_codec::header(
                                        child,
                                        limits.max_cell_bytes,
                                        crate::formula_codec::HeaderPolicy::KnownRecords,
                                    )
                                    .map_err(|error| error.with_cell(cell.address))?;
                                    if matches!(
                                        metadata.kind,
                                        crabxl_core::FormulaType::Shared { .. }
                                    ) {
                                        return Err(Error::new(ErrorKind::Unsupported, "Replacing shared formula records requires group normalization").with_cell(cell.address));
                                    }
                                }
                            }
                            Event::Eof => return Err(invalid("Unexpected end of replaced cell")),
                            _ => {}
                        }
                    }
                    emit(&mut writer, Event::Start(start))?;
                    write_body(&mut writer, cell, &mut buffer, limits, formula_attributes)?;
                    let name = std::str::from_utf8(&name).map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid cell name", error)
                    })?;
                    emit(
                        &mut writer,
                        Event::End(quick_xml::events::BytesEnd::new(name)),
                    )?;
                    in_cell = false;
                    found += 1;
                } else if selected_row {
                    let address = CellAddress::new(row, next_column - 1)?;
                    let start = positioned_start(&e, &address.to_string(), false)?;
                    emit(&mut writer, Event::Start(start))?;
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::Start(e)
                if in_cell
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 5
                    && e.local_name().as_ref().as_bytes() == b"f" =>
            {
                if seen_v {
                    return Err(invalid("Formula follows its cached value"));
                }
                formula = true;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if in_cell
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 5
                    && e.local_name().as_ref().as_bytes() == b"v" =>
            {
                seen_v = true;
                if formula && invalidate_caches {
                    loop {
                        let frame = xml.next()?;
                        if matches!(&frame.event,Event::End(end) if frame.depth==4 && end.local_name().as_ref().as_bytes()==b"v")
                        {
                            break;
                        }
                        if matches!(frame.event, Event::Eof) {
                            return Err(invalid("Unexpected end of formula cache"));
                        }
                    }
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::End(e)
                if in_cell
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"c" =>
            {
                in_cell = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::End(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"row" =>
            {
                while pending
                    .peek()
                    .is_some_and(|patch| patch.cell.address.row.get() == row)
                {
                    let patch = pending
                        .next()
                        .ok_or_else(|| invalid("Missing pending cell"))?;
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    write_inserted_cell(
                        &mut writer,
                        patch,
                        uri,
                        &mut buffer,
                        limits,
                        formula_attributes,
                    )?;
                    found += 1;
                }
                in_row = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::End(e)
                if in_data
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"sheetData" =>
            {
                while let Some(patch) = pending.peek() {
                    let inserted_row = patch.cell.address.row.get();
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    let mut start = BytesStart::new("row");
                    let reference = (inserted_row + 1).to_string();
                    start.push_attribute(("r", reference.as_str()));
                    start.push_attribute(("xmlns", uri));
                    emit(&mut writer, Event::Start(start))?;
                    while pending
                        .peek()
                        .is_some_and(|patch| patch.cell.address.row.get() == inserted_row)
                    {
                        let patch = pending
                            .next()
                            .ok_or_else(|| invalid("Missing pending cell"))?;
                        write_inserted_cell(
                            &mut writer,
                            patch,
                            uri,
                            &mut buffer,
                            limits,
                            formula_attributes,
                        )?;
                        found += 1;
                    }
                    emit(
                        &mut writer,
                        Event::End(quick_xml::events::BytesEnd::new("row")),
                    )?;
                }
                in_data = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    if !seen_data {
        return Err(invalid("Affected worksheet has no sheetData"));
    }
    if patches.is_some_and(|patches| found != patches.len()) {
        return Err(invalid(
            "Pending replacement targets a missing physical cell",
        ));
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}
