//! Values operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Append a complete scalar/formula row after actual source/pending extent.
    /// The selected sheet materializes once; advertised dimensions do not choose
    /// the append position. Validate all values and joint model/overlay/scratch
    /// allowances before committing either representation. Date and unresolved
    /// phonetic-font assignments retain their explicit unsupported errors.
    pub fn append(&mut self, id: SheetId, values: Vec<CellValue>) -> Result<RowIndex> {
        self.sheet(id)?;
        let source = self
            .sheets
            .iter()
            .find(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if source.original.is_none() || self.editor.model_is_dirty(&source.name) {
            let created = source.original.is_none();
            let row = RowIndex::new(self.bank.sheet(id)?.row_extent())?;
            for (column, value) in values.iter().enumerate() {
                let address = CellAddress::new(row.get(), column as u32)?;
                if created {
                    self.editor.validate_created_value(address, value)?;
                } else {
                    self.editor.prepare_value(&source.name, address, value)?;
                }
            }
            let result = self.bank.sheet_mut(id)?.append(values);
            if created && result.is_ok() {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let sheet = self.bank.sheet(id)?;
        let row = RowIndex::new(sheet.row_extent())?;
        let increase = sheet
            .preflight_append(&values)?
            .saturating_sub(sheet.charged_bytes());
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let planning_bytes = values
            .len()
            .saturating_mul(std::mem::size_of::<crate::editor::PatchPlan>());
        let planning_retained = self
            .mapping_bytes()
            .saturating_add(self.package_extra_bytes())
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(planning_bytes);
        self.editor
            .book
            .rebalance_strings_for_retained(planning_retained, maximum)?;
        let scratch_allowance = maximum.saturating_sub(self.managed_retained_bytes());
        let plan = self
            .editor
            .prepare_row(&source.name, row, &values, scratch_allowance)?;
        let package = self
            .package_extra_bytes()
            .saturating_sub(self.editor.patch_bytes())
            .saturating_add(plan.bytes);
        let scratch = plan.scratch_bytes.saturating_add(
            values
                .len()
                .saturating_mul(std::mem::size_of::<CellValue>()),
        );
        let retained = self
            .mapping_bytes()
            .saturating_add(package)
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(increase)
            .saturating_add(scratch);
        self.editor
            .book
            .rebalance_strings_for_retained(retained, maximum)?;
        let available = maximum
            .checked_sub(
                self.mapping_bytes()
                    .saturating_add(package)
                    .saturating_add(self.editor.book.retained_source_bytes())
                    .saturating_add(scratch),
            )
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)?;
        if self.bank.remaining_bytes() < increase {
            self.rebalance()?;
            return Err(budget());
        }
        let result = self.bank.sheet_mut(id)?.append(values.clone());
        if let Err(error) = result {
            self.rebalance()?;
            return Err(error);
        }
        self.editor.commit_row(plan, values);
        self.rebalance()?;
        Ok(row)
    }
    pub(super) fn edit_value(
        &mut self,
        id: SheetId,
        address: CellAddress,
        value: CellValue,
        insert_missing: bool,
    ) -> Result<()> {
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if source.original.is_none() {
            self.editor.validate_created_value(address, &value)?;
            if !insert_missing && self.bank.sheet(id)?.get(address).is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Replacement targets a missing model cell",
                )
                .with_cell(address));
            }
            let maximum = self
                .allowance
                .retained_data_bytes
                .min(self.options.workbook.max_bytes);
            let retained = self
                .mapping_bytes()
                .saturating_add(self.package_extra_bytes())
                .saturating_add(self.bank.charged_bytes())
                .saturating_add(value.heap_bytes());
            self.editor
                .book
                .rebalance_strings_for_retained(retained, maximum)?;
            self.rebalance()?;
            let style = self
                .bank
                .sheet(id)?
                .get(address)
                .map_or(crabxl_core::StyleId::new(0), |cell| cell.style);
            let result = self.bank.sheet_mut(id)?.set(Cell {
                address,
                value,
                style,
            });
            if result.is_ok() {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let plan = self.editor.prepare_value(&source.name, address, &value)?;
        let loaded = source.loaded;
        let model_dirty = self.editor.model_is_dirty(&source.name);
        if model_dirty && !insert_missing && self.bank.sheet(id)?.get(address).is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Replacement targets a missing model cell",
            )
            .with_cell(address));
        }
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let package = self
            .package_extra_bytes()
            .saturating_sub(self.editor.patch_bytes())
            .saturating_add(plan.bytes);
        let retained = self
            .mapping_bytes()
            .saturating_add(package)
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(if loaded { value.heap_bytes() } else { 0 });
        self.editor
            .book
            .rebalance_strings_for_retained(retained, maximum)?;
        let available = maximum
            .checked_sub(
                self.mapping_bytes()
                    .saturating_add(package)
                    .saturating_add(self.editor.book.retained_source_bytes()),
            )
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)?;
        if loaded {
            if self.bank.remaining_bytes() < value.heap_bytes() {
                self.rebalance()?;
                return Err(budget());
            }
            let style = self
                .bank
                .sheet(id)?
                .get(address)
                .map_or(crabxl_core::StyleId::new(0), |cell| cell.style);
            let result = self.bank.sheet_mut(id)?.set(Cell {
                address,
                value: value.clone(),
                style,
            });
            if let Err(error) = result {
                self.rebalance()?;
                return Err(error);
            }
        }
        if !model_dirty {
            self.editor.commit_value(plan, value, insert_missing);
        }
        self.rebalance()
    }
}
