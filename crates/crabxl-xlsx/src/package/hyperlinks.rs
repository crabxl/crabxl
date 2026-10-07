//! Relationship-resolved hyperlink metadata without materializing worksheet XML.
use super::*;

impl<R: Read + Seek> WorkbookReader<R> {
    /// Read point declarations and resolved relationship targets in a bounded scan.
    /// Range declarations reject typed access instead of allocating a dense grid.
    pub fn hyperlinks(&mut self, name: &str) -> Result<crabxl_core::Hyperlinks> {
        self.hyperlinks_with_allowance(
            name,
            self.limits.max_metadata_bytes.min(usize::MAX as u64) as usize,
        )
    }
    pub(crate) fn hyperlinks_with_allowance(
        &mut self,
        name: &str,
        maximum: usize,
    ) -> Result<crabxl_core::Hyperlinks> {
        let maximum = maximum.min(self.limits.max_metadata_bytes.min(usize::MAX as u64) as usize);
        let part = self.hyperlink_sheet_part(name)?;
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
        let mut capture = crate::hyperlinks::Capture::default();
        loop {
            let frame = xml.next()?;
            if let Event::Start(e) = &frame.event
                && frame.depth == 1
                && (frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"worksheet")
            {
                return Err(invalid("Invalid hyperlink worksheet root").with_part(&part));
            }
            capture
                .observe(&frame, maximum, |_| Ok(()))
                .map_err(|error| error.with_part(&part))?;
            if matches!(frame.event, Event::Eof) {
                break;
            }
        }
        drop(xml);
        self.resolve_hyperlinks(name, capture.links, maximum)
    }
    /// Replay declarations only when normalized coverage cannot recover original
    /// first-fill value order. Ordinary non-overlapping loads keep one cell pass.
    pub(crate) fn visit_hyperlinks_in_source_order(
        &mut self,
        name: &str,
        maximum: usize,
        mut visit: impl FnMut(crabxl_core::CellRange, &crabxl_core::Hyperlink, usize) -> Result<()>,
    ) -> Result<()> {
        let part = self.hyperlink_sheet_part(name)?;
        let relation_part = relationship_part(&part);
        let limits = self.limits;
        let working = limits
            .input_buffer_bytes
            .saturating_add(limits.max_xml_event_bytes.saturating_mul(2));
        let available = maximum.checked_sub(working).ok_or_else(|| {
            Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Ordered hyperlink scan exceeds allowance",
            )
        })?;
        let mut remaining =
            available.min(limits.max_metadata_bytes.min(usize::MAX as u64) as usize) as u64;
        let initial = remaining;
        let relationships = if self.archive.file_names().any(|name| name == relation_part) {
            read_relationships(&mut self.archive, &relation_part, limits, &mut remaining)?
        } else {
            HashMap::new()
        };
        let workspace = working.saturating_add((initial - remaining) as usize);
        let file = self.archive.by_name(&part).map_err(|cause| {
            Error::caused_by(
                ErrorKind::Archive,
                "Cannot open ordered hyperlink worksheet",
                cause,
            )
            .with_part(&part)
        })?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, file),
            part.clone(),
            limits.max_part_bytes,
            limits,
        );
        let mut capture = crate::hyperlinks::Capture::default();
        loop {
            let frame = xml.next()?;
            if let Event::Start(e) = &frame.event
                && frame.depth == 1
                && (frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"worksheet")
            {
                return Err(invalid("Invalid hyperlink worksheet root").with_part(&part));
            }
            capture
                .observe_with(
                    &frame,
                    remaining as usize,
                    |_| Ok(()),
                    |range, mut link| {
                        let target = if let Some(id) = link.relationship_id.as_deref() {
                            let relation = relationships.get(id).ok_or_else(|| {
                                invalid("Unknown hyperlink relationship").with_cell(range.start)
                            })?;
                            if !relationship_is(&relation.kind, "hyperlink") {
                                return Err(invalid("Hyperlink relationship has a different type")
                                    .with_cell(range.start));
                            }
                            Some(relation)
                        } else {
                            None
                        };
                        let charge = link
                            .heap_bytes()
                            .saturating_add(size_of::<crabxl_core::Hyperlink>())
                            .saturating_add(target.map_or(0, |relation| relation.target.len()));
                        if charge > remaining as usize {
                            return Err(Error::new(
                                ErrorKind::MemoryBudgetExceeded,
                                "Ordered hyperlink declaration exceeds allowance",
                            )
                            .with_cell(range.start));
                        }
                        if let Some(relation) = target {
                            link.target = Some(relation.target.clone().into_boxed_str());
                            link.external = relation.external;
                        }
                        visit(range, &link, workspace.saturating_add(charge))?;
                        Ok(None)
                    },
                )
                .map_err(|error| error.with_part(&part))?;
            if matches!(frame.event, Event::Eof) {
                return Ok(());
            }
        }
    }

    fn hyperlink_sheet_part(&self, name: &str) -> Result<String> {
        let sheet = self
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet not found"))?;
        if sheet.kind != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Non-cell-sheet hyperlinks remain unimplemented",
            ));
        }
        Ok(sheet.part.clone())
    }
    /// Resolve captured declarations after their row-stream borrow has ended.
    /// Ownership transfer keeps partially resolved metadata private on failure.
    pub fn resolve_hyperlinks(
        &mut self,
        name: &str,
        mut links: crabxl_core::Hyperlinks,
        maximum: usize,
    ) -> Result<crabxl_core::Hyperlinks> {
        let maximum = maximum.min(self.limits.max_metadata_bytes.min(usize::MAX as u64) as usize);
        if links.heap_bytes() > maximum {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Captured hyperlinks exceed resolution allowance",
            ));
        }
        let part = self.hyperlink_sheet_part(name)?;
        if !links.iter().any(|(_, link)| link.relationship_id.is_some()) {
            return Ok(links);
        }
        let relationship_part = relationship_part(&part);
        let available = maximum.checked_sub(links.heap_bytes()).ok_or_else(|| {
            Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Captured hyperlinks exceed resolution allowance",
            )
        })?;
        let count = links
            .iter()
            .filter(|(_, link)| link.relationship_id.is_some())
            .count();
        let scratch = count
            .checked_mul(size_of::<crabxl_core::CellAddress>())
            .ok_or_else(|| limit("Hyperlink resolution scratch overflows"))?;
        if scratch > available {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Hyperlink resolution scratch exceeds allowance",
            ));
        }
        let mut addresses = Vec::new();
        addresses.try_reserve_exact(count).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot reserve hyperlink resolution scratch",
                cause,
            )
        })?;
        addresses.extend(
            links
                .iter()
                .filter(|(_, link)| link.relationship_id.is_some())
                .map(|(address, _)| address),
        );
        let mut remaining = available
            .saturating_sub(addresses.capacity() * size_of::<crabxl_core::CellAddress>())
            as u64;
        let relationships = read_relationships(
            &mut self.archive,
            &relationship_part,
            self.limits,
            &mut remaining,
        )?;
        let link_maximum = links
            .heap_bytes()
            .saturating_add(remaining.min(usize::MAX as u64) as usize);
        for address in addresses {
            let id = links
                .get(address)
                .and_then(|link| link.relationship_id.as_deref())
                .ok_or_else(|| invalid("Missing retained hyperlink relationship"))?;
            let relation = relationships.get(id).ok_or_else(|| {
                invalid("Unknown hyperlink relationship")
                    .with_part(&part)
                    .with_cell(address)
            })?;
            if !relationship_is(&relation.kind, "hyperlink") {
                return Err(invalid("Hyperlink relationship has a different type")
                    .with_part(&part)
                    .with_cell(address));
            }
            let prospective = links
                .heap_bytes()
                .saturating_sub(
                    links
                        .get(address)
                        .and_then(|link| link.target.as_ref())
                        .map_or(0, |value| value.len()),
                )
                .saturating_add(relation.target.len());
            if prospective > link_maximum {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Resolved hyperlink target exceeds allowance",
                )
                .with_part(&part)
                .with_cell(address));
            }
            links.set_relationship_target(
                address,
                relation.target.clone().into_boxed_str(),
                relation.external,
                link_maximum,
            )?;
        }
        Ok(links)
    }
}
