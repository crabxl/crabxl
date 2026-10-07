//! Relationship-resolved hyperlink metadata without materializing worksheet XML.
use super::*;

impl<R: Read + Seek> WorkbookReader<R> {
    /// Read point declarations and resolved relationship targets in a bounded scan.
    /// Range declarations reject typed access instead of allocating a dense grid.
    pub fn hyperlinks(&mut self, name: &str) -> Result<crabxl_core::Hyperlinks> {
        let sheet = self
            .sheets
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet not found"))?;
        if sheet.kind != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Non-cell-sheet hyperlinks remain unimplemented",
            ));
        }
        let part = sheet.part.clone();
        let rels = relationship_part(&part);
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = if self.archive.file_names().any(|name| name == rels) {
            read_relationships(&mut self.archive, &rels, self.limits, &mut remaining)?
        } else {
            HashMap::new()
        };
        let file = self.archive.by_name(&part).map_err(|cause| {
            Error::caused_by(ErrorKind::Archive, "Cannot open hyperlink worksheet", cause)
                .with_part(&part)
        })?;
        let limits = self.limits;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, file),
            part.clone(),
            limits.max_part_bytes,
            limits,
        );
        let mut result = crabxl_core::Hyperlinks::default();
        let mut seen = false;
        let mut active = false;
        loop {
            let frame = xml.next()?;
            match &frame.event {
                Event::Start(e)
                    if frame.depth == 1
                        && (frame.scope != Scope::Spreadsheet
                            || e.local_name().as_ref().as_bytes() != b"worksheet") =>
                {
                    return Err(invalid("Invalid hyperlink worksheet root").with_part(&part));
                }
                Event::Start(e)
                    if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"hyperlinks" =>
                {
                    if seen || frame.scope != Scope::Spreadsheet {
                        return Err(
                            invalid("Invalid or duplicate hyperlinks container").with_part(&part)
                        );
                    }
                    seen = true;
                    active = true;
                    for a in e.attributes() {
                        let a = a.map_err(|_| invalid("Invalid hyperlink container attribute"))?;
                        if a.key.as_namespace_binding().is_none() {
                            return Err(Error::new(
                                ErrorKind::Unsupported,
                                "Unknown hyperlink container attributes",
                            )
                            .with_part(&part));
                        }
                    }
                }
                Event::Start(e) if active => {
                    if frame.depth != 3
                        || frame.scope != Scope::Spreadsheet
                        || e.local_name().as_ref().as_bytes() != b"hyperlink"
                    {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Unknown hyperlink content",
                        )
                        .with_part(&part));
                    }
                    for (index, a) in e.attributes().enumerate() {
                        let a = a.map_err(|_| invalid("Invalid hyperlink attribute"))?;
                        if a.key.as_namespace_binding().is_none()
                            && frame.office_relationship_attribute != Some(index)
                            && !matches!(
                                a.key.as_ref().as_bytes(),
                                b"ref" | b"location" | b"display" | b"tooltip"
                            )
                        {
                            return Err(Error::new(
                                ErrorKind::Unsupported,
                                "Unknown hyperlink attributes",
                            )
                            .with_part(&part));
                        }
                    }
                    let range: crabxl_core::CellRange = required_attribute(e, b"ref")?.parse()?;
                    if range.start != range.end {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Range hyperlink editing remains unimplemented",
                        )
                        .with_part(&part));
                    }
                    if result.get(range.start).is_some() {
                        return Err(invalid("Duplicate hyperlink coordinate")
                            .with_part(&part)
                            .with_cell(range.start));
                    }
                    let mut link = crabxl_core::Hyperlink {
                        location: attribute(e, b"location")?.map(String::into_boxed_str),
                        display: attribute(e, b"display")?.map(String::into_boxed_str),
                        tooltip: attribute(e, b"tooltip")?.map(String::into_boxed_str),
                        relationship_id: frame.office_relationship.as_deref().map(Into::into),
                        ..Default::default()
                    };
                    if let Some(id) = &frame.office_relationship {
                        let relation = relationships.get(id).ok_or_else(|| {
                            invalid("Unknown hyperlink relationship").with_part(&part)
                        })?;
                        if !relationship_is(&relation.kind, "hyperlink") {
                            return Err(invalid("Hyperlink relationship has a different type")
                                .with_part(&part));
                        }
                        link.target = Some(relation.target.clone().into_boxed_str());
                        link.external = relation.external;
                    }
                    result.set(
                        range.start,
                        Some(link),
                        remaining.min(usize::MAX as u64) as usize,
                    )?;
                }
                Event::End(e)
                    if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"hyperlinks" =>
                {
                    active = false
                }
                Event::Text(t)
                    if active && !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
                {
                    return Err(invalid("Unexpected hyperlink text").with_part(&part));
                }
                Event::CData(_) | Event::GeneralRef(_) | Event::Comment(_) | Event::PI(_)
                    if active =>
                {
                    return Err(
                        Error::new(ErrorKind::Unsupported, "Unknown hyperlink payload")
                            .with_part(&part),
                    );
                }
                Event::Eof => return Ok(result),
                _ => {}
            }
        }
    }
}
