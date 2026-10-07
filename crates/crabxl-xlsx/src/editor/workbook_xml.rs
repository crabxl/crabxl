//! Workbook catalog, view and shared-string XML rewriting.
use super::*;

pub(super) struct WorkbookRewrite<'a> {
    pub(super) limits: ResourceLimits,
    pub(super) invalidate_caches: bool,
    pub(super) active: Option<crabxl_core::ActiveViewSelection>,
    pub(super) visibility: &'a BTreeMap<usize, crabxl_core::SheetVisibility>,
    pub(super) names: &'a BTreeMap<usize, Box<str>>,
    pub(super) order: Option<&'a CatalogOrder>,
    pub(super) membership: Option<(&'a catalog::Membership, &'a crabxl_core::Workbook)>,
}
pub(super) fn rewritten_catalog_entry(
    original: &BytesStart<'_>,
    name: Option<&str>,
    state: Option<crabxl_core::SheetVisibility>,
) -> Result<BytesStart<'static>> {
    let mut start = original.to_owned();
    start.clear_attributes();
    for attribute in original.attributes() {
        let attribute = attribute.map_err(|cause| {
            Error::caused_by(ErrorKind::Xml, "Invalid sheet catalog attribute", cause)
        })?;
        if !(state.is_some() && attribute.key.as_ref().as_bytes() == b"state"
            || name.is_some() && attribute.key.as_ref().as_bytes() == b"name")
        {
            start.push_attribute(attribute);
        }
    }
    if let Some(state) = state {
        start.push_attribute(("state", state.as_str()));
    }
    if let Some(name) = name {
        start.push_attribute(("name", name));
    }
    Ok(start)
}
pub(super) fn patch_workbook<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    rewrite: WorkbookRewrite<'_>,
) -> Result<u64> {
    let WorkbookRewrite {
        limits,
        invalidate_caches,
        active,
        visibility,
        names,
        order,
        membership,
    } = rewrite;
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let mut seen = false;
    let mut uri = None;
    let mut views_seen = false;
    let mut views_open = false;
    let mut active_written = false;
    let mut sheet_index = 0usize;
    let mut sheets_open = false;
    let mut skipped_sheet = false;
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if skipped_sheet {
            match &frame.event {
                Event::End(_) if frame.depth == 2 => skipped_sheet = false,
                Event::Text(text)
                    if text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
                _ => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    ));
                }
            }
            continue;
        }
        if frame.scope == Scope::Spreadsheet {
            match &frame.event {
                Event::Start(e)
                    if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = true
                }
                Event::End(e)
                    if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = false
                }
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
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"workbook"
                {
                    return Err(invalid("Affected part is not a workbook"));
                }
                uri = frame.spreadsheet_uri;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if sheets_open
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"sheet" =>
            {
                if order.is_some() || membership.is_some() {
                    sheet_index += 1;
                    skipped_sheet = true;
                    continue;
                }
                let state = visibility.get(&sheet_index);
                let name = names.get(&sheet_index);
                sheet_index += 1;
                if state.is_some() || name.is_some() {
                    let start =
                        rewritten_catalog_entry(&e, name.map(AsRef::as_ref), state.copied())?;
                    emit(&mut writer, Event::Start(start))?;
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"sheets"
                    && (order.is_some() || membership.is_some()) =>
            {
                if let Some((membership, bank)) = membership {
                    if sheet_index != membership.declarations.len() {
                        return Err(invalid("Original sheet catalog changed"));
                    }
                    membership.write_entries(&mut writer, bank)?;
                } else if let Some(order) = order {
                    if sheet_index != order.entries.len() {
                        return Err(invalid("Original sheet catalog changed"));
                    }
                    for &source in &order.positions {
                        let original = order
                            .entries
                            .get(source)
                            .ok_or_else(|| invalid("Original sheet order is inconsistent"))?;
                        let state = visibility.get(&source).copied();
                        let name = names.get(&source).map(AsRef::as_ref);
                        if state.is_some() || name.is_some() {
                            let start = rewritten_catalog_entry(&original.start, name, state)?;
                            emit(&mut writer, Event::Empty(start))?;
                        } else {
                            emit(&mut writer, Event::Empty(original.start.borrow()))?;
                        }
                    }
                }
                emit(&mut writer, Event::End(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"bookViews" =>
            {
                views_seen = true;
                views_open = true;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if active.is_some()
                    && views_open
                    && !active_written
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"workbookView" =>
            {
                let mut start = e.to_owned();
                start.clear_attributes();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid workbook view attribute", error)
                    })?;
                    if attribute.key.as_ref().as_bytes() != b"activeTab" {
                        start.push_attribute(attribute);
                    }
                }
                let value = active
                    .and_then(|view| view.serialized_index)
                    .map(|index| index.to_string());
                if let Some(value) = &value {
                    start.push_attribute(("activeTab", value.as_str()));
                }
                emit(&mut writer, Event::Start(start))?;
                active_written = true;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"bookViews" =>
            {
                if active.is_some() && !active_written {
                    emit_active_view(&mut writer, uri, active)?;
                    active_written = true;
                }
                views_open = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::Start(e)
                if active.is_some()
                    && !views_seen
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheets" =>
            {
                let mut start = BytesStart::new("bookViews");
                start.push_attribute((
                    "xmlns",
                    uri.ok_or_else(|| invalid("Workbook namespace is missing"))?,
                ));
                emit(&mut writer, Event::Start(start))?;
                emit_active_view(&mut writer, uri, active)?;
                emit(
                    &mut writer,
                    Event::End(quick_xml::events::BytesEnd::new("bookViews")),
                )?;
                views_seen = true;
                active_written = true;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if invalidate_caches
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"calcPr" =>
            {
                if seen {
                    return Err(invalid("Duplicate calculation properties"));
                }
                seen = true;
                let mut start = e.to_owned();
                start.clear_attributes();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid calculation attribute", error)
                    })?;
                    if !matches!(
                        attribute.key.as_ref().as_bytes(),
                        b"calcMode" | b"fullCalcOnLoad" | b"forceFullCalc"
                    ) {
                        start.push_attribute(attribute);
                    }
                }
                start.push_attribute(("calcMode", "auto"));
                start.push_attribute(("fullCalcOnLoad", "1"));
                start.push_attribute(("forceFullCalc", "1"));
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Start(e)
                if invalidate_caches
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && matches!(
                        e.local_name().as_ref().as_bytes(),
                        b"oleSize"
                            | b"customWorkbookViews"
                            | b"pivotCaches"
                            | b"smartTagPr"
                            | b"smartTagTypes"
                            | b"webPublishing"
                            | b"fileRecoveryPr"
                            | b"webPublishObjects"
                            | b"extLst"
                    ) =>
            {
                if !seen {
                    emit_calculation(&mut writer, uri)?;
                    seen = true;
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 0
                    && e.local_name().as_ref().as_bytes() == b"workbook" =>
            {
                if invalidate_caches && !seen {
                    emit_calculation(&mut writer, uri)?;
                    seen = true;
                }
                emit(&mut writer, Event::End(e))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    if invalidate_caches && !seen {
        return Err(invalid("Workbook calculation properties were not written"));
    }
    if active.is_some() && !active_written {
        return Err(invalid("Workbook active view was not written"));
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}
pub(super) fn emit_active_view<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    uri: Option<&str>,
    active: Option<crabxl_core::ActiveViewSelection>,
) -> Result<()> {
    let mut start = BytesStart::new("workbookView");
    start.push_attribute((
        "xmlns",
        uri.ok_or_else(|| invalid("Workbook namespace is missing"))?,
    ));
    let value = active
        .and_then(|view| view.serialized_index)
        .map(|index| index.to_string());
    if let Some(value) = &value {
        start.push_attribute(("activeTab", value.as_str()));
    }
    emit(writer, Event::Empty(start))
}
pub(super) fn emit_calculation<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    uri: Option<&str>,
) -> Result<()> {
    let mut start = BytesStart::new("calcPr");
    start.push_attribute((
        "xmlns",
        uri.ok_or_else(|| invalid("Workbook namespace is missing"))?,
    ));
    start.push_attribute(("calcMode", "auto"));
    start.push_attribute(("fullCalcOnLoad", "1"));
    start.push_attribute(("forceFullCalc", "1"));
    emit(writer, Event::Empty(start))
}

pub(super) fn patch_shared_strings<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    limits: ResourceLimits,
) -> Result<u64> {
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let mut root = false;
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet || e.local_name().as_ref().as_bytes() != b"sst"
                {
                    return Err(invalid("Shared string part has an invalid root"));
                }
                let mut start = e.to_owned();
                start.clear_attributes();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid shared string attribute", error)
                    })?;
                    if attribute.key.as_ref().as_bytes() != b"count" {
                        start.push_attribute(attribute);
                    }
                }
                root = true;
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    if !root {
        return Err(invalid("Shared string part has no root"));
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}
