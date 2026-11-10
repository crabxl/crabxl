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
    if let Some(annotations) = value.annotations() {
        for (name, value) in [
            ("cm", &annotations.cell_metadata),
            ("vm", &annotations.value_metadata),
        ] {
            if let Some(value) = value {
                start.push_attribute((name, value.as_ref()));
            }
        }
    }
    let stored = value.storage_value()?;
    let literal = match stored.as_ref() {
        CellValue::Formula(formula) => formula.cached(),
        value => Some(value),
    };
    match literal {
        Some(CellValue::Text(_) | CellValue::RichText(_)) => start.push_attribute((
            "t",
            if matches!(stored.as_ref(), CellValue::Formula(_))
                || value.storage_type() == Some(crabxl_core::CellStorageType::FormulaString)
            {
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
#[allow(clippy::too_many_arguments)]
pub(super) fn write_body<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    cell: &Cell,
    buffer: &mut RowBuffer,
    limits: ResourceLimits,
    formula_attributes: crate::FormulaWritePolicy,
    metadata_counts: Option<(usize, usize)>,
    epoch: DateEpoch,
    iso_dates: bool,
) -> Result<()> {
    // The caller writes the style in the cell header; the body does not use it.
    let unstyled = (cell.style.get() != 0).then(|| Cell {
        address: cell.address,
        value: cell.value.clone(),
        style: crabxl_core::StyleId::new(0),
    });
    let cell = unstyled.as_ref().unwrap_or(cell);
    encode_cells(
        buffer,
        cell.address.row,
        std::slice::from_ref(cell).iter(),
        limits.max_cell_bytes,
        1,
        StyleContext::Appearance(&[CellStyle::default()]),
        ValueEncoding {
            metadata_counts,
            epoch,
            iso_dates,
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
#[allow(clippy::too_many_arguments)]
pub(super) fn write_inserted_cell<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    patch: &Patch,
    uri: &str,
    buffer: &mut RowBuffer,
    limits: ResourceLimits,
    formula_attributes: crate::FormulaWritePolicy,
    metadata_counts: Option<(usize, usize)>,
    epoch: DateEpoch,
    iso_dates: bool,
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
    let mut start = patched_start(&base, uri, &patch.cell.value)?;
    if patch.cell.style.get() != 0 {
        start.push_attribute(("s", patch.cell.style.get().to_string().as_str()));
    }
    emit(writer, Event::Start(start))?;
    write_body(
        writer,
        &patch.cell,
        buffer,
        limits,
        formula_attributes,
        metadata_counts,
        epoch,
        iso_dates,
    )?;
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
    pub(super) features: Option<(
        &'a crabxl_core::SheetFeatures,
        &'a crate::features::sheet::PartRefs,
    )>,
    pub(super) hyperlinks: Option<(
        &'a crabxl_core::Hyperlinks,
        &'a crate::hyperlinks::source::Plan,
    )>,
    pub(super) invalidate_caches: bool,
    pub(super) model: Option<&'a crabxl_core::Worksheet>,
    pub(super) catalog: Option<&'a crabxl_core::StyleCatalog>,
    pub(super) epoch: DateEpoch,
    pub(super) iso_dates: bool,
    pub(super) non_finite: crate::NonFiniteWritePolicy,
    pub(super) copying: bool,
    pub(super) data_only: bool,
    pub(super) metadata_counts: Option<(usize, usize)>,
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
        features,
        hyperlinks,
        invalidate_caches,
        model,
        catalog,
        epoch,
        iso_dates,
        non_finite,
        copying,
        data_only,
        metadata_counts,
    } = rewrite;
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let mut print_rewrite = match (printing, features) {
        (printing, Some((features, _))) => Some(crate::printing::Rewrite::with_header_footer(
            printing,
            Some(features.header_footer.as_ref()),
        )),
        (Some(printing), None) => Some(crate::printing::Rewrite::new(printing)),
        (None, None) => None,
    };
    let mut early_features = features.map(|(features, refs)| {
        crate::features::sheet::Rewrite::new(
            features,
            refs,
            0,
            crate::features::sheet::hyperlinks_rank(),
        )
    });
    let mut late_features = features.map(|(features, refs)| {
        crate::features::sheet::Rewrite::new(
            features,
            refs,
            crate::features::sheet::after_printing_rank(),
            usize::MAX,
        )
    });
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
    // Shared groups whose master cell was replaced. As in openpyxl, their
    // followers are written as ordinary formulas translated from the master.
    let mut orphans: std::collections::HashMap<
        crabxl_core::SharedFormulaIndex,
        (String, CellAddress),
    > = std::collections::HashMap::new();
    let mut current_cell: Option<CellAddress> = None;
    let mut buffer = RowBuffer {
        data: Vec::new(),
        maximum: limits.max_row_bytes,
    };
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if let Some(rewrite) = &mut early_features
            && frame.scope == Scope::Spreadsheet
        {
            match &frame.event {
                Event::Start(e) if frame.depth == 2 => {
                    let skip = rewrite
                        .before_start(
                            writer.get_mut(),
                            e.local_name().as_ref().as_bytes(),
                            frame.spreadsheet_uri,
                        )
                        .map_err(|cause| io_error("Cannot replace worksheet features", cause))?;
                    if skip {
                        crate::style_codec::skip(&mut xml, 2)?;
                        continue;
                    }
                }
                Event::End(_) if frame.depth == 0 => rewrite
                    .finish(writer.get_mut(), frame.spreadsheet_uri)
                    .map_err(|cause| io_error("Cannot finish worksheet features", cause))?,
                _ => {}
            }
        }
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
            if !copying
                && !in_data
                && frame.depth >= 2
                && frame.scope == Scope::MarkupCompatibility
                && model.is_none_or(|model| !model.has_structural_edits())
                && views.is_none()
                && printing.is_none()
                && features.is_none()
                && hyperlinks.is_none()
            {
                let alternative_depth = frame.depth;
                // Buffer the block: alternatives holding cell data are dropped
                // whole, as openpyxl does, since the edit cannot reach them.
                let mut events = vec![frame.event.into_owned()];
                let mut keep = true;
                loop {
                    let nested = xml.next()?;
                    if matches!(nested.event, Event::Eof) {
                        return Err(invalid("Unexpected end of worksheet alternative"));
                    }
                    let finished = matches!(nested.event, Event::End(_))
                        && nested.depth + 1 == alternative_depth;
                    if nested.scope == Scope::Spreadsheet
                        && matches!(&nested.event, Event::Start(start) if matches!(start.local_name().as_ref().as_bytes(), b"sheetData" | b"row" | b"c" | b"f"))
                    {
                        keep = false;
                    }
                    if keep {
                        events.push(nested.event.into_owned());
                    }
                    if finished {
                        break;
                    }
                }
                if keep {
                    for event in events {
                        writer.write_event(event).map_err(|cause| {
                            io_error("Cannot preserve worksheet alternative", cause)
                        })?;
                    }
                }
                continue;
            }
            // Other alternatives are dropped, as openpyxl does.
            let depth = frame.depth;
            crate::style_codec::skip(&mut xml, depth)?;
            continue;
        }
        // openpyxl's public copy behavior omits views and header/footer state.
        if copying
            && frame.depth == 1
            && frame.scope == Scope::Spreadsheet
            && let Event::Start(e) = &frame.event
        {
            let mut start = e.to_owned();
            start.clear_attributes();
            for attribute in e.attributes() {
                let attribute = attribute.map_err(|cause| {
                    Error::caused_by(ErrorKind::Xml, "Invalid worksheet attribute", cause)
                })?;
                if !crate::xml::namespaced_attribute(
                    frame.namespaces,
                    attribute.key,
                    crate::xml::EXCEL_REVISION,
                    b"uid",
                ) {
                    start.push_attribute(attribute);
                }
            }
            emit(&mut writer, Event::Start(start))?;
            continue;
        }
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
        if let Some(rewrite) = &mut late_features
            && frame.scope == Scope::Spreadsheet
        {
            match &frame.event {
                Event::Start(e) if frame.depth == 2 => {
                    let skip = rewrite
                        .before_start(
                            writer.get_mut(),
                            e.local_name().as_ref().as_bytes(),
                            frame.spreadsheet_uri,
                        )
                        .map_err(|cause| io_error("Cannot replace worksheet features", cause))?;
                    if skip {
                        crate::style_codec::skip(&mut xml, 2)?;
                        continue;
                    }
                }
                Event::End(_) if frame.depth == 0 => rewrite
                    .finish(writer.get_mut(), frame.spreadsheet_uri)
                    .map_err(|cause| io_error("Cannot finish worksheet features", cause))?,
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
        if let Some(rewrite) = &mut print_rewrite
            && let Event::Start(e) = &frame.event
            && frame.scope == Scope::Spreadsheet
        {
            let skip = rewrite
                .format(
                    writer.get_mut(),
                    e.local_name().as_ref().as_bytes(),
                    frame.depth,
                    frame.spreadsheet_uri,
                )
                .map_err(|error| io_error("Cannot replace sheet format", error))?;
            if skip {
                let depth = frame.depth;
                crate::style_codec::skip(&mut xml, depth)?;
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
                            metadata_counts,
                            epoch,
                            iso_dates,
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
                            ErrorKind::InvalidData,
                            "Merged cells other than the top-left one are read-only",
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
                            metadata_counts,
                            epoch,
                            iso_dates,
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
                        metadata_counts,
                        epoch,
                        iso_dates,
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
                            metadata_counts,
                            epoch,
                            iso_dates,
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
                                    // Unknown content of the replaced cell is dropped.
                                    continue;
                                }
                                if child.local_name().as_ref().as_bytes() == b"f" {
                                    let metadata = crate::formula_codec::header(
                                        child,
                                        limits.max_cell_bytes,
                                        crate::formula_codec::HeaderPolicy::KnownRecords,
                                    )
                                    .map_err(|error| error.with_cell(cell.address))?;
                                    if let crabxl_core::FormulaType::Shared { index, .. } =
                                        &metadata.kind
                                        && !data_only
                                        && metadata.reference.is_some()
                                    {
                                        let mut expression = String::new();
                                        loop {
                                            let text = xml.next()?;
                                            match &text.event {
                                                Event::End(_) => break,
                                                Event::Eof => {
                                                    return Err(invalid(
                                                        "Unexpected end of formula",
                                                    ));
                                                }
                                                event => crate::xml::append_xml_text(
                                                    &mut expression,
                                                    event,
                                                    limits.max_cell_bytes,
                                                )?,
                                            }
                                        }
                                        orphans.insert(index.clone(), (expression, cell.address));
                                    }
                                }
                            }
                            Event::Eof => return Err(invalid("Unexpected end of replaced cell")),
                            _ => {}
                        }
                    }
                    emit(&mut writer, Event::Start(start))?;
                    write_body(
                        &mut writer,
                        cell,
                        &mut buffer,
                        limits,
                        formula_attributes,
                        metadata_counts,
                        epoch,
                        iso_dates,
                    )?;
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
                    current_cell = Some(address);
                    let start = positioned_start(&e, &address.to_string(), false)?;
                    emit(&mut writer, Event::Start(start))?;
                } else {
                    current_cell = if orphans.is_empty() {
                        None
                    } else {
                        attribute(&e, b"r")?
                            .map(|value| value.parse::<CellAddress>())
                            .transpose()?
                    };
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
                if data_only {
                    crate::style_codec::skip(&mut xml, 5)?;
                } else if let Some(expression) =
                    orphan_formula(&e, current_cell, &orphans, limits.max_cell_bytes)?
                {
                    let name = e.name().as_ref().to_owned();
                    let mut start = quick_xml::events::BytesStart::new(name.clone());
                    for attribute in e.attributes().flatten() {
                        if !matches!(attribute.key.as_ref(), "t" | "si" | "ref") {
                            start.push_attribute(attribute);
                        }
                    }
                    crate::style_codec::skip(&mut xml, 5)?;
                    emit(&mut writer, Event::Start(start))?;
                    emit(
                        &mut writer,
                        Event::Text(quick_xml::events::BytesText::new(&expression)),
                    )?;
                    emit(
                        &mut writer,
                        Event::End(quick_xml::events::BytesEnd::new(name)),
                    )?;
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::Start(e)
                if in_cell
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 5
                    && e.local_name().as_ref().as_bytes() == b"v" =>
            {
                seen_v = true;
                if formula && invalidate_caches && !data_only {
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
                        metadata_counts,
                        epoch,
                        iso_dates,
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
                            metadata_counts,
                            epoch,
                            iso_dates,
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

/// The translated text for a follower whose shared master was replaced.
fn orphan_formula(
    e: &quick_xml::events::BytesStart<'_>,
    address: Option<CellAddress>,
    orphans: &std::collections::HashMap<crabxl_core::SharedFormulaIndex, (String, CellAddress)>,
    maximum: usize,
) -> Result<Option<String>> {
    let Some(address) = address.filter(|_| !orphans.is_empty()) else {
        return Ok(None);
    };
    let metadata =
        crate::formula_codec::header(e, maximum, crate::formula_codec::HeaderPolicy::KnownRecords)?;
    let crabxl_core::FormulaType::Shared { index, .. } = &metadata.kind else {
        return Ok(None);
    };
    let Some((expression, anchor)) = orphans.get(index) else {
        return Ok(None);
    };
    crabxl_core::translate_expression(
        expression,
        i64::from(address.row.get()) - i64::from(anchor.row.get()),
        i64::from(address.column.get()) - i64::from(anchor.column.get()),
        maximum,
    )
    .map(Some)
}
