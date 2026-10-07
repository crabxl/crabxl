//! Overlays.
use super::*;

impl<R: Read + Seek> WorkbookEditor<R> {
    /// Replace an existing cell's value, preserving its style and unrelated cell
    /// attributes. Cell existence and unsupported metadata are checked on save.
    /// Original dates/shared/rich strings can be retained opaquely; creating a
    /// typed date here requires the later read-side style catalog.
    pub fn set_value(&mut self, sheet: &str, address: CellAddress, value: CellValue) -> Result<()> {
        self.queue_value(sheet, address, value, false)
    }
    /// Replace an existing cell or insert a missing physical cell. Existing
    /// cells retain their style; new cells use the default style. Insertions
    /// update an existing dimension and make inferred coordinates explicit.
    /// Non-anchor cells of merged ranges are rejected during save.
    pub fn upsert_value(
        &mut self,
        sheet: &str,
        address: CellAddress,
        value: CellValue,
    ) -> Result<()> {
        self.queue_value(sheet, address, value, true)
    }
    pub(super) fn queue_value(
        &mut self,
        sheet: &str,
        address: CellAddress,
        value: CellValue,
        insert_missing: bool,
    ) -> Result<()> {
        let plan = self.prepare_value(sheet, address, &value)?;
        self.commit_value(plan, value, insert_missing);
        Ok(())
    }
    pub(super) fn editable_sheet(&self, sheet: &str) -> Result<usize> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing a digitally signed package requires an explicit signature policy",
            ));
        }
        if !self.chain_safe
            || (!self.calc_chain_parts.is_empty()
                && self.options.calculation_chain == CalculationChainPolicy::RejectEdits)
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Calculation-chain edits are rejected by policy or unsupported incoming relationships",
            ));
        }
        let sheet = self
            .book
            .sheets()
            .iter()
            .position(|info| info.name() == sheet)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let info = &self.book.sheets()[sheet];
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Selected sheet is not a cell worksheet",
            ));
        }
        Ok(sheet)
    }
    pub(crate) fn validate_style_edit(&self, catalog: &crabxl_core::StyleCatalog) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Signed style editing remains unimplemented",
            ));
        }
        if self.book.style_part.is_none() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            ));
        }
        if self.styles_dirty {
            Ok(())
        } else {
            crate::styles::validate_catalog(catalog)
        }
    }
    pub(crate) fn validate_theme_edit(&self) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Signed theme editing remains unimplemented",
            ));
        }
        if self.book.theme_part.is_none() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source theme relationship remains unimplemented",
            ));
        }
        Ok(())
    }
    pub(crate) fn theme_is_dirty(&self) -> bool {
        self.theme_dirty
    }
    pub(crate) fn theme_changed(&mut self) {
        self.theme_dirty = true;
    }
    pub(crate) fn styles_changed(&mut self) {
        self.styles_dirty = true;
    }
    pub(crate) fn model_is_dirty(&self, sheet: &str) -> bool {
        self.book
            .sheets()
            .iter()
            .position(|info| info.name() == sheet)
            .is_some_and(|index| self.model_patches.contains_key(&index))
    }
    pub(crate) fn prepare_model(&mut self, sheet: &str) -> Result<ModelPlan> {
        let index = self.editable_sheet(sheet)?;
        self.guard_workbook_graphs()?;
        let part = self.book.sheets()[index].part().to_owned();
        let old = self.patches.get(&part).map_or(0, |patches| {
            PATCH_BYTES
                + part.len()
                + patches
                    .values()
                    .map(|patch| PATCH_BYTES.saturating_add(patch.cell.value.heap_bytes()))
                    .sum::<usize>()
        });
        let bytes = self.patch_bytes.saturating_sub(old).saturating_add(
            if self.model_patches.contains_key(&index) {
                0
            } else {
                PATCH_BYTES
            },
        );
        if bytes > self.options.max_patch_bytes {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Source model rewrite ledger allowance exceeded",
            ));
        }
        if !self.model_patches.contains_key(&index) {
            let file =
                self.book.archive.by_name(&part).map_err(|cause| {
                    zip_error("Cannot inspect structural source worksheet", cause)
                })?;
            let limits = self.options.resources;
            let mut xml = XmlStream::new(
                BufReader::with_capacity(limits.input_buffer_bytes, file),
                part.clone(),
                limits.max_part_bytes,
                limits,
            );
            let shared_strings = crate::loaded_codec::guard(
                &mut xml,
                self.structural_inline_rich_text,
                limits.max_cell_bytes,
            )
            .map_err(|error| error.with_part(&part))?;
            drop(xml);
            if shared_strings && !self.structural_plain_strings {
                if let Some(part) = self.book.source_strings_part().map(str::to_owned) {
                    let file = self.book.archive.by_name(&part).map_err(|cause| {
                        zip_error("Cannot inspect structural shared strings", cause)
                    })?;
                    let mut xml = XmlStream::new(
                        BufReader::with_capacity(limits.input_buffer_bytes, file),
                        part.clone(),
                        limits.max_part_bytes,
                        limits,
                    );
                    crate::loaded_codec::guard_strings(
                        &mut xml,
                        self.structural_rich_text,
                        limits.max_cell_bytes,
                    )
                    .map_err(|error| error.with_part(&part))?;
                }
                self.structural_plain_strings = true;
            }
        }
        Ok(ModelPlan {
            sheet: index,
            bytes,
        })
    }
    pub(crate) fn commit_model(&mut self, plan: ModelPlan, id: crabxl_core::SheetId) {
        let part = self.book.sheets()[plan.sheet].part();
        if let Some(patches) = self.patches.remove(part) {
            self.patch_cells -= patches.len();
        }
        self.model_patches.insert(plan.sheet, id);
        self.patch_bytes = plan.bytes;
    }
    pub(crate) fn validate_cell_value(
        &self,
        address: CellAddress,
        value: &CellValue,
    ) -> Result<()> {
        if contains_date(value) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing dates requires an existing style/date catalog",
            )
            .with_cell(address));
        }
        if let CellValue::RichText(value) = value
            && let Some(properties) = &value.phonetic_properties
        {
            let fonts = self.model_font_count.ok_or_else(|| {
                Error::new(
                    ErrorKind::Unsupported,
                    "Editing phonetic font references requires the imported font catalog",
                )
                .with_cell(address)
            })?;
            if properties.font_id as usize >= fonts {
                return Err(
                    Error::new(ErrorKind::InvalidData, "Unknown phonetic font identity")
                        .with_cell(address),
                );
            }
        }
        let epoch = if self.book.date_1904() {
            DateEpoch::Mac1904
        } else {
            DateEpoch::Windows1900
        };
        crate::encode::validate_non_finite(value, self.options.non_finite)
            .map_err(|error| error.with_cell(address))?;
        validate_value(value, self.options.resources.max_cell_bytes, epoch)
            .map_err(|error| error.with_cell(address))?;
        Ok(())
    }
    pub(crate) fn validate_created_value(
        &self,
        address: CellAddress,
        value: &CellValue,
    ) -> Result<()> {
        if self.signed
            || !self.chain_safe
            || !self.calc_chain_parts.is_empty()
                && self.options.calculation_chain == CalculationChainPolicy::RejectEdits
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Created value edits are rejected by signature/calculation policy",
            ));
        }
        self.validate_cell_value(address, value)
    }
    pub(crate) fn prepare_value(
        &self,
        sheet: &str,
        address: CellAddress,
        value: &CellValue,
    ) -> Result<PatchPlan> {
        let sheet = self.editable_sheet(sheet)?;
        let info = &self.book.sheets()[sheet];
        self.validate_cell_value(address, value)?;
        if self.model_patches.contains_key(&sheet) {
            return Ok(PatchPlan {
                sheet,
                address,
                bytes: self.patch_bytes,
                cells: self.patch_cells,
            });
        }
        let key = (address.row.get(), address.column.get());
        let old = self
            .patches
            .get(info.part())
            .and_then(|patches| patches.get(&key));
        let old_bytes = old.map_or(0, |cell| {
            PATCH_BYTES.saturating_add(cell.cell.value.heap_bytes())
        });
        let new_part = !self.patches.contains_key(info.part());
        let bytes = self
            .patch_bytes
            .saturating_sub(old_bytes)
            .saturating_add(PATCH_BYTES)
            .saturating_add(value.heap_bytes())
            .saturating_add(if new_part {
                PATCH_BYTES + info.part().len()
            } else {
                0
            });
        let cells = self.patch_cells + usize::from(old.is_none());
        if bytes > self.options.max_patch_bytes || cells > self.options.max_patch_cells {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Pending cell overlay allowance exceeded",
            )
            .with_cell(address));
        }
        Ok(PatchPlan {
            sheet,
            address,
            bytes,
            cells,
        })
    }
    pub(crate) fn commit_value(&mut self, plan: PatchPlan, value: CellValue, insert_missing: bool) {
        let part = self.book.sheets()[plan.sheet].part();
        let key = (plan.address.row.get(), plan.address.column.get());
        self.patches.entry(part.into()).or_default().insert(
            key,
            Patch {
                cell: Cell {
                    address: plan.address,
                    value,
                    style: StyleId::new(0),
                },
                insert_missing,
            },
        );
        self.patch_bytes = plan.bytes;
        self.patch_cells = plan.cells;
    }
    /// Validate the entire row and its combined overlay charge before mutation.
    pub(crate) fn prepare_row(
        &self,
        sheet: &str,
        row: crabxl_core::RowIndex,
        values: &[CellValue],
        scratch_allowance: usize,
    ) -> Result<RowPatchPlan> {
        if values.len() > crabxl_core::MAX_COLUMNS as usize {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Appended row exceeds column bounds",
            ));
        }
        if values
            .len()
            .saturating_mul(std::mem::size_of::<PatchPlan>())
            > scratch_allowance
        {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Row edit planning allowance exceeded",
            ));
        }
        let mut plans = Vec::new();
        plans.try_reserve_exact(values.len()).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate row edit plan",
                cause,
            )
        })?;
        let mut bytes = self.patch_bytes;
        let mut cells = self.patch_cells;
        for (column, value) in values.iter().enumerate() {
            let address = CellAddress::new(row.get(), column as u32)?;
            let mut plan = self.prepare_value(sheet, address, value)?;
            let part = self.book.sheets()[plan.sheet].part();
            let repeated_part = if !plans.is_empty() && !self.patches.contains_key(part) {
                PATCH_BYTES + part.len()
            } else {
                0
            };
            bytes = bytes.saturating_add(
                plan.bytes
                    .saturating_sub(self.patch_bytes)
                    .saturating_sub(repeated_part),
            );
            cells = cells.saturating_add(plan.cells.saturating_sub(self.patch_cells));
            if bytes > self.options.max_patch_bytes || cells > self.options.max_patch_cells {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Pending row overlay allowance exceeded",
                )
                .with_cell(address));
            }
            plan.bytes = bytes;
            plan.cells = cells;
            plans.push(plan);
        }
        let scratch_bytes = plans
            .capacity()
            .saturating_mul(std::mem::size_of::<PatchPlan>());
        Ok(RowPatchPlan {
            plans,
            bytes,
            scratch_bytes,
        })
    }
    pub(crate) fn commit_row(&mut self, plan: RowPatchPlan, values: Vec<CellValue>) {
        for (patch, value) in plan.plans.into_iter().zip(values) {
            self.commit_value(patch, value, true);
        }
    }
    /// Inspect only a pending replacement, without decoding the original cell.
    pub fn pending_value(&self, sheet: &str, address: CellAddress) -> Option<&CellValue> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)?
            .part();
        self.patches
            .get(part)?
            .get(&(address.row.get(), address.column.get()))
            .map(|patch| &patch.cell.value)
    }
    /// Borrow pending cells for one original sheet in row/column order.
    /// Style IDs here are placeholders; original styles are resolved on save.
    pub fn pending_cells(&self, sheet: &str) -> impl Iterator<Item = &Cell> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .map(SheetInfo::part);
        part.into_iter()
            .flat_map(|part| self.patches.get(part))
            .flat_map(|patches| patches.values())
            .map(|patch| &patch.cell)
    }
    /// Revert all overlays to the original source; releases owned payloads.
    /// This does not adopt a previously saved file as the new source.
    pub fn clear_edits(&mut self) {
        self.patches = BTreeMap::new();
        self.view_patches = BTreeMap::new();
        self.print_patches = BTreeMap::new();
        self.active_patch = None;
        self.visibility_patches = BTreeMap::new();
        self.name_patches = BTreeMap::new();
        self.catalog_order = None;
        self.model_patches = BTreeMap::new();
        self.hyperlink_patches = BTreeMap::new();
        self.membership = None;
        self.patch_bytes = 0;
        self.patch_cells = 0;
    }
}
