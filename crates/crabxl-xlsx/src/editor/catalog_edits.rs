//! Catalog edits.
use super::*;

impl<R: Read + Seek> WorkbookEditor<R> {
    /// Rename an original catalog entry without changing its part or relationship
    /// identity. The selector is its original source name, even after a rename.
    /// Formula expressions and defined-name text retain reference behavior: they
    /// are not automatically rewritten when a sheet title changes.
    pub fn rename_sheet(&mut self, source_name: &str, name: impl Into<Box<str>>) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == source_name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let name = name.into();
        let planned = self.prepare_name(index, &name)?;
        self.commit_name(index, name, planned);
        Ok(())
    }
    pub(crate) fn prepare_name(&mut self, index: usize, name: &str) -> Result<usize> {
        crate::encode::validate_catalog_name(name)?;
        let folded = name.to_lowercase();
        if self
            .book
            .sheets()
            .iter()
            .enumerate()
            .any(|(position, sheet)| {
                position != index
                    && self
                        .name_patches
                        .get(&position)
                        .map_or(sheet.name(), |value| value.as_ref())
                        .to_lowercase()
                        == folded
            })
        {
            return Err(invalid("Duplicate worksheet name"));
        }
        let previous = self.name_patches.get(&index);
        let bytes = self
            .patch_bytes
            .saturating_sub(previous.map_or(0, |name| name.len()))
            .saturating_add(if previous.is_none() { PATCH_BYTES } else { 0 })
            .saturating_add(name.len());
        self.validate_workbook_patch(index, bytes, false)?;
        Ok(bytes)
    }
    pub(crate) fn commit_name(&mut self, index: usize, name: Box<str>, bytes: usize) {
        self.name_patches.insert(index, name);
        self.patch_bytes = bytes;
    }
    /// Reorder an original sheet to a zero-based display position, retaining its
    /// source part and the signed active-view index. Local defined-name graphs
    /// remain an explicit staged dependency and reject before catalog mutation.
    pub fn move_sheet(&mut self, source_name: &str, position: usize) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == source_name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let available = self
            .options
            .max_patch_bytes
            .saturating_sub(self.patch_bytes);
        let plan = self.prepare_order(index, position, available)?;
        self.commit_order(plan);
        Ok(())
    }
    pub(crate) fn prepare_order(
        &mut self,
        index: usize,
        position: usize,
        scratch_allowance: usize,
    ) -> Result<OrderPlan> {
        let count = self.book.sheets().len();
        if index >= count || position >= count {
            return Err(invalid("Sheet position is out of range"));
        }
        self.validate_workbook_patch(index, self.patch_bytes, false)?;
        let scratch_bytes = count.saturating_mul(size_of::<usize>());
        if scratch_bytes > scratch_allowance {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Sheet order planning allowance exceeded",
            ));
        }
        let mut positions = Vec::new();
        positions.try_reserve_exact(count).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate sheet order",
                cause,
            )
        })?;
        positions.extend((0..count).map(|display| self.source_index(display)));
        let old = positions
            .iter()
            .position(|source| *source == index)
            .ok_or_else(|| invalid("Original sheet order is inconsistent"))?;
        positions.remove(old);
        positions.insert(position, index);
        let entries = if self.catalog_order.is_none() {
            Some(self.read_catalog_entries(scratch_allowance.saturating_sub(scratch_bytes))?)
        } else {
            None
        };
        let charged = entries.as_ref().map_or_else(
            || self.catalog_order.as_ref().map_or(0, |order| order.charged),
            |entries| catalog_order_bytes(&positions, entries),
        );
        let previous = self.catalog_order.as_ref().map_or(0, |order| order.charged);
        let bytes = self
            .patch_bytes
            .saturating_sub(previous)
            .saturating_add(charged)
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        if bytes > self.options.max_patch_bytes {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Sheet order patch allowance exceeded",
            ));
        }
        Ok(OrderPlan {
            positions,
            entries,
            bytes,
            scratch_bytes,
            view_index: self.active_view_index(),
        })
    }
    pub(super) fn read_catalog_entries(
        &mut self,
        allowance: usize,
    ) -> Result<Vec<SheetDeclaration>> {
        let part = self.book.workbook_part.clone();
        let count = self.book.sheets().len();
        let mut entries = Vec::new();
        let fixed = PATCH_BYTES
            .saturating_add(count.saturating_mul(size_of::<SheetDeclaration>()))
            .saturating_add(count.saturating_mul(size_of::<usize>()));
        if fixed > allowance {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Sheet catalog planning allowance exceeded",
            ));
        }
        entries.try_reserve_exact(count).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate original sheet catalog",
                cause,
            )
        })?;
        let input = self
            .book
            .archive
            .by_name(&part)
            .map_err(|cause| zip_error("Cannot inspect original sheet order", cause))?;
        let limits = self.options.resources;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, input),
            part.clone(),
            limits.max_metadata_bytes.min(limits.max_part_bytes),
            limits,
        );
        let mut charged = fixed;
        let mut sheets_open = false;
        let mut sheet_open = false;
        let mut names_open = false;
        loop {
            let frame = xml.next()?;
            check_declaration(&frame.event)?;
            match &frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"definedNames" =>
                {
                    names_open = true
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 1
                        && e.local_name().as_ref().as_bytes() == b"definedNames" =>
                {
                    names_open = false
                }
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = true
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 1
                        && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = false
                }
                Event::Start(e) if sheets_open && frame.depth == 3 => {
                    if frame.scope != Scope::Spreadsheet
                        || e.local_name().as_ref().as_bytes() != b"sheet"
                    {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Sheet catalog extensions require typed reorder handling",
                        )
                        .with_part(&part));
                    }
                    charged = charged
                        .saturating_add(e.as_ref().len())
                        .saturating_add(frame.office_relationship.as_ref().map_or(0, String::len));
                    if charged > allowance || entries.len() == count {
                        return Err(Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Sheet catalog planning allowance exceeded",
                        )
                        .with_part(&part));
                    }
                    for attribute in e.attributes() {
                        attribute.map_err(|cause| {
                            Error::caused_by(
                                ErrorKind::Xml,
                                "Invalid sheet catalog attribute",
                                cause,
                            )
                            .with_part(&part)
                        })?;
                    }
                    entries.push(SheetDeclaration {
                        start: e.to_owned(),
                        relationship: frame.office_relationship.as_deref().map(Into::into),
                        namespace: frame
                            .spreadsheet_uri
                            .ok_or_else(|| invalid("Missing sheet declaration namespace"))?,
                    });
                    sheet_open = true;
                }
                Event::Start(_) if sheet_open => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    )
                    .with_part(&part));
                }
                Event::End(_) if sheet_open && frame.depth == 2 => sheet_open = false,
                Event::Text(text)
                    if sheet_open
                        && !text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    )
                    .with_part(&part));
                }
                Event::CData(_) | Event::GeneralRef(_) | Event::Comment(_) | Event::PI(_)
                    if sheet_open =>
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    )
                    .with_part(&part));
                }
                Event::Start(e)
                    if names_open
                        && frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref().as_bytes() == b"definedName"
                        && attribute(e, b"localSheetId")?.is_some() =>
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Local defined-name graph reordering remains unimplemented",
                    )
                    .with_part(&part));
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if entries.len() != count {
            return Err(invalid("Original sheet catalog changed").with_part(&part));
        }
        Ok(entries)
    }
    pub(crate) fn commit_order(&mut self, plan: OrderPlan) {
        if let Some(entries) = plan.entries {
            let charged = catalog_order_bytes(&plan.positions, &entries);
            self.catalog_order = Some(Box::new(CatalogOrder {
                positions: plan.positions,
                entries,
                charged,
            }));
        } else if let Some(order) = &mut self.catalog_order {
            order.positions = plan.positions;
        }
        self.active_patch = Some(ActivePatch::Deferred(plan.view_index));
        self.patch_bytes = plan.bytes;
    }
    pub(super) fn source_index(&self, display: usize) -> usize {
        self.catalog_order
            .as_ref()
            .map_or(display, |order| order.positions[display])
    }
    /// Select a visible original worksheet/chartsheet without decoding its cells.
    /// Only workbook view metadata changes; formula caches/chains stay intact.
    pub fn set_active_sheet(&mut self, name: &str) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Active sheet does not exist"))?;
        let index = (0..self.book.sheets().len())
            .find(|display| self.source_index(*display) == index)
            .ok_or_else(|| invalid("Original sheet order is inconsistent"))?;
        let bytes = self.prepare_active(index)?;
        self.commit_active(index, bytes);
        Ok(())
    }
    pub(crate) fn prepare_active(&mut self, index: usize) -> Result<usize> {
        let bytes = self
            .patch_bytes
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        self.validate_workbook_patch(index, bytes, true)?;
        Ok(bytes)
    }
    /// Set a deferred workbook view. Relative, hidden and out-of-range indexes
    /// are resolved when saving, independently of strict visible-ID selection.
    pub fn set_active_view_index(&mut self, index: i64) -> Result<()> {
        let bytes = self.prepare_active_view(index)?;
        self.commit_active_view(index, bytes);
        Ok(())
    }
    pub(crate) fn prepare_active_view(&mut self, index: i64) -> Result<usize> {
        let bytes = self
            .patch_bytes
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        let position =
            crabxl_core::resolve_sheet_index(index, self.book.sheets().len()).unwrap_or(0);
        self.validate_workbook_patch(position, bytes, false)?;
        Ok(bytes)
    }
    pub(crate) fn commit_active_view(&mut self, index: i64, bytes: usize) {
        self.active_patch = Some(ActivePatch::Deferred(index));
        self.patch_bytes = bytes;
    }
    /// Pending signed view, or the original declaration when unchanged.
    pub fn active_view_index(&self) -> i64 {
        match self.active_patch {
            Some(ActivePatch::Visible(index)) => index as i64,
            Some(ActivePatch::Deferred(index)) => index,
            None => self.book.active_view_index(),
        }
    }
    /// Effective catalog visibility without decoding any worksheet cells.
    pub fn sheet_visibility(&self, name: &str) -> Result<crabxl_core::SheetVisibility> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        Ok(self.visibility_at_source(index))
    }
    /// Change an original sheet's catalog state without changing its contents.
    /// An all-hidden intermediate model is permitted; saving rejects it before
    /// writing output, so callers can restore a visible sheet and retry.
    pub fn set_sheet_visibility(
        &mut self,
        name: &str,
        visibility: crabxl_core::SheetVisibility,
    ) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let bytes = self.prepare_visibility(index)?;
        self.commit_visibility(index, visibility, bytes);
        Ok(())
    }
    pub(crate) fn prepare_visibility(&mut self, index: usize) -> Result<usize> {
        let bytes = self
            .patch_bytes
            .saturating_add(if self.visibility_patches.contains_key(&index) {
                0
            } else {
                PATCH_BYTES
            })
            // Reserve the fixed active overlay now, so successful save can
            // normalize a newly hidden selection without growing the ledger.
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        self.validate_workbook_patch(index, bytes, false)?;
        Ok(bytes)
    }
    pub(crate) fn commit_visibility(
        &mut self,
        index: usize,
        visibility: crabxl_core::SheetVisibility,
        bytes: usize,
    ) {
        self.visibility_patches.insert(index, visibility);
        if self.active_patch.is_none() {
            self.active_patch = Some(ActivePatch::Visible(self.book.active_index().unwrap_or(0)));
        }
        self.patch_bytes = bytes;
    }
    pub(super) fn visibility_at(&self, index: usize) -> crabxl_core::SheetVisibility {
        self.visibility_at_source(self.source_index(index))
    }
    pub(super) fn visibility_at_source(&self, index: usize) -> crabxl_core::SheetVisibility {
        self.visibility_patches
            .get(&index)
            .copied()
            .unwrap_or_else(|| self.book.sheets()[index].visibility())
    }
    pub(crate) fn active_index(&self) -> Option<usize> {
        crabxl_core::resolve_sheet_index(self.active_view_index(), self.book.sheets().len())
    }
    pub(super) fn active_for_save(&self) -> Result<Option<crabxl_core::ActiveViewSelection>> {
        if self.active_patch.is_none() && self.visibility_patches.is_empty() {
            return Ok(None);
        }
        if let Some(ActivePatch::Deferred(index)) = self.active_patch {
            return crabxl_core::normalize_active_view(
                index,
                self.book.sheets().len(),
                |position| self.visibility_at(position),
            )
            .map(Some);
        }
        let mut visible = (0..self.book.sheets().len())
            .filter(|index| self.visibility_at(*index) == crabxl_core::SheetVisibility::Visible);
        let first = visible
            .clone()
            .next()
            .ok_or_else(|| invalid("A workbook requires at least one visible sheet"))?;
        let active = self.active_index().unwrap_or(first);
        let index = visible.find(|index| *index >= active).unwrap_or(first) as i64;
        Ok(Some(crabxl_core::ActiveViewSelection {
            serialized_index: Some(index),
            requested_index: index,
        }))
    }
    pub(super) fn validate_workbook_patch(
        &mut self,
        index: usize,
        bytes: usize,
        require_visible: bool,
    ) -> Result<()> {
        let count = self.book.sheets().len();
        if index >= count {
            return Err(Error::new(
                ErrorKind::SheetNotFound,
                "Active sheet does not exist",
            ));
        }
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed packages is unsupported",
            ));
        }
        if require_visible && self.visibility_at(index) != crabxl_core::SheetVisibility::Visible {
            return Err(invalid("Active sheet must be visible"));
        }
        if bytes > self.options.max_patch_bytes {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Workbook metadata patch allowance exceeded",
            ));
        }
        let part = self.book.workbook_part.clone();
        let input = self
            .book
            .archive
            .by_name(&part)
            .map_err(|error| zip_error("Cannot inspect active sheet metadata", error))?;
        let limits = self.options.resources;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, input),
            part.clone(),
            limits.max_metadata_bytes.min(limits.max_part_bytes),
            limits,
        );
        let mut sheet_index = 0usize;
        let mut sheets_open = false;
        loop {
            let frame = xml.next()?;
            check_declaration(&frame.event)?;
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
            match frame.event {
                Event::Start(e) if e.local_name().as_ref().as_bytes() == b"AlternateContent" => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Editing markup-compatibility alternatives requires typed branch handling",
                    )
                    .with_part(&part));
                }
                Event::Start(e)
                    if sheets_open
                        && frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref().as_bytes() == b"sheet" =>
                {
                    sheet_index += 1;
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if sheet_index != count {
            return Err(invalid("Original sheet catalog changed").with_part(&part));
        }
        Ok(())
    }
    pub(crate) fn commit_active(&mut self, index: usize, bytes: usize) {
        self.active_patch = Some(ActivePatch::Visible(index));
        self.patch_bytes = bytes;
    }
}
