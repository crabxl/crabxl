//! Package relationship and content-type graph surgery.
use super::*;

pub(super) enum GraphOwner<'a> {
    CalculationChain,
    Worksheet(&'a str),
}
impl GraphOwner<'_> {
    fn role(&self) -> &str {
        match self {
            Self::CalculationChain => "calcChain",
            Self::Worksheet(_) => "worksheet",
        }
    }
    fn accepts_id(&self, event: &BytesStart<'_>) -> Result<bool> {
        Ok(match self {
            Self::CalculationChain => true,
            Self::Worksheet(id) => attribute(event, b"Id")?.as_deref() == Some(*id),
        })
    }
}
pub(super) fn catalog_part_removal<R: Read + Seek>(
    book: &mut WorkbookReader<R>,
    parts: &[PartInfo],
    targets: &HashSet<String>,
    workbook_relationships: &str,
    limits: ResourceLimits,
    used: &mut usize,
    owner: GraphOwner<'_>,
) -> Result<(HashSet<String>, bool)> {
    // Retain only tiny owned-path inventories, not target cell data or a
    // package relationship DOM. Additional graph scans share one byte cap.
    let mut removals = HashSet::new();
    for target in targets {
        for name in [target.clone(), crate::package::relationship_part(target)] {
            if !parts.iter().any(|part| part.name.as_ref() == name) {
                continue;
            }
            *used = used.saturating_add(name.len()).saturating_add(128);
            if *used as u128 > u128::from(limits.max_metadata_bytes) {
                return Err(limit("Package removal inventory budget exceeded"));
            }
            removals.insert(name);
        }
    }
    let mut safe = targets
        .iter()
        .all(|chain| parts.iter().any(|part| part.name.as_ref() == chain));
    let mut remaining = limits.max_metadata_bytes;
    for part in parts {
        // Check declared owner edges, including missing/misdeclared targets.
        // Other incoming consumers matter when deleting cataloged parts.
        if part.name.as_ref() != workbook_relationships && targets.is_empty() {
            continue;
        }
        let Some(source) = crate::package::relationship_source(&part.name) else {
            continue;
        };
        if part.uncompressed_bytes > remaining {
            return Err(
                limit("Package removal relationship scan byte limit exceeded")
                    .with_part(part.name.as_ref()),
            );
        }
        let file = book
            .archive
            .by_name(&part.name)
            .map_err(|error| zip_error("Cannot inspect package removal relationships", error))?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, file),
            part.name.to_string(),
            remaining,
            limits,
        );
        loop {
            let frame = xml.next()?;
            if !targets.is_empty()
                && matches!(&frame.event, Event::Start(e) if e.local_name().as_ref().as_bytes()==b"AlternateContent")
            {
                safe = false;
            }
            match frame.event {
                Event::Start(e) if frame.depth == 1 => {
                    if frame.scope != Scope::Relationships
                        || e.local_name().as_ref().as_bytes() != b"Relationships"
                    {
                        safe = false;
                    }
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Relationships
                        && e.local_name().as_ref().as_bytes() == b"Relationship" =>
                {
                    let kind = attribute(&e, b"Type")?
                        .ok_or_else(|| invalid("Relationship has no type"))?;
                    if matches!(owner, GraphOwner::Worksheet(_))
                        && kind
                            == "http://schemas.microsoft.com/office/2006/relationships/vbaProject"
                    {
                        // VBA can name sheets without an OPC edge to their parts.
                        safe = false;
                    }
                    let target = attribute(&e, b"Target")?
                        .ok_or_else(|| invalid("Relationship has no target"))?;
                    if removals.contains(part.name.as_ref()) {
                        // Outgoing internal or external edges belong to a graph
                        // requiring a typed deletion policy.
                        safe = false;
                    }
                    if attribute(&e, b"TargetMode")?.as_deref() == Some("External") {
                        if crate::package::relationship_is(&kind, owner.role()) {
                            safe = false;
                        }
                        continue;
                    }
                    let target = crate::package::resolve_part(&source, &target)?;
                    if matches!(owner, GraphOwner::CalculationChain)
                        && crate::package::relationship_is(&kind, owner.role())
                        && (part.name.as_ref() != workbook_relationships
                            || !targets.contains(&target))
                    {
                        safe = false;
                    }
                    if targets.contains(&target)
                        && !(part.name.as_ref() == workbook_relationships
                            && crate::package::relationship_is(&kind, owner.role())
                            && owner.accepts_id(&e)?)
                    {
                        // Unknown consumers must not be left with dangling refs.
                        safe = false;
                    }
                }
                Event::Start(_) if !targets.is_empty() => {
                    // Unknown relationship-container extensions can own hidden
                    // consumers; typed graph inspection must precede disposal.
                    safe = false;
                }
                Event::Text(text)
                    if !text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
                {
                    safe = false;
                }
                Event::CData(_) | Event::GeneralRef(_) => safe = false,
                Event::Eof => break,
                _ => {}
            }
        }
        remaining = remaining.saturating_sub(xml.bytes_consumed());
    }
    Ok((removals, safe))
}
pub(super) fn patch_chain_metadata<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    chains: &HashSet<String>,
    limits: ResourceLimits,
    membership: Option<(&catalog::Membership, &crabxl_core::Workbook)>,
) -> Result<u64> {
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_metadata_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let types = part == "[Content_Types].xml";
    let mut skip = None;
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if let Some(depth) = skip {
            if matches!(&frame.event, Event::End(_)) && frame.depth == depth - 1 {
                skip = None;
            }
            if matches!(&frame.event, Event::Eof) {
                return Err(invalid("Truncated removed package declaration"));
            }
            continue;
        }
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                let valid = if types {
                    frame.scope == Scope::ContentTypes
                        && e.local_name().as_ref().as_bytes() == b"Types"
                } else {
                    frame.scope == Scope::Relationships
                        && e.local_name().as_ref().as_bytes() == b"Relationships"
                };
                if !valid {
                    return Err(invalid("Invalid calculation-chain package metadata root"));
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.depth == 2
                    && types
                    && frame.scope == Scope::ContentTypes
                    && e.local_name().as_ref().as_bytes() == b"Override" =>
            {
                let name = attribute(&e, b"PartName")?
                    .ok_or_else(|| invalid("Content override has no part name"))?;
                let name = crate::package::resolve_part("", &name)?;
                if chains.contains(&name)
                    || membership.is_some_and(|(member, _)| member.removes_part(&name))
                {
                    skip = Some(frame.depth);
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::Start(e)
                if frame.depth == 2
                    && !types
                    && frame.scope == Scope::Relationships
                    && e.local_name().as_ref().as_bytes() == b"Relationship" =>
            {
                let kind =
                    attribute(&e, b"Type")?.ok_or_else(|| invalid("Relationship has no type"))?;
                let removed = if let Some((member, _)) = membership {
                    let id = attribute(&e, b"Id")?
                        .ok_or_else(|| invalid("Relationship has no identifier"))?;
                    member.removes_relationship(&id)
                } else {
                    false
                };
                if removed
                    || (!chains.is_empty() && crate::package::relationship_is(&kind, "calcChain"))
                {
                    skip = Some(frame.depth);
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::End(e) if frame.depth == 0 => {
                if let Some((membership, bank)) = membership {
                    membership.write_additions(&mut writer, types, bank)?;
                }
                emit(&mut writer, Event::End(e))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}
