//! Values operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Append a complete scalar/formula row after actual source/pending extent.
    /// The selected sheet materializes once; advertised dimensions do not choose
    /// the append position. Validate all values and joint model/overlay/scratch
    /// allowances before committing either representation. Temporal rows use
    /// the canonical model and register their workbook-local number formats.
    pub fn append(&mut self, id: SheetId, values: Vec<CellValue>) -> Result<RowIndex> {
        let values = match self.append_streamed(id, values)? {
            Ok(row) => return Ok(row),
            Err(values) => values,
        };
        if values.iter().any(|value| value.temporal_value().is_some()) {
            self.prepare_style_edit()?;
            self.sheet(id)?;
            let row = self.edit_structure_when(id, |sheet| sheet.append(values), |_| true)?;
            self.editor.styles_changed();
            self.rebalance()?;
            return Ok(row);
        }
        self.editor.model_font_count = self.bank.style_catalog().map(|catalog| catalog.fonts.len());
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
            .prepare_row(&source.name, row, &values, scratch_allowance, false)?;
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
        self.editor
            .commit_row(plan, values, |_| crabxl_core::StyleId::new(0));
        self.rebalance()?;
        Ok(row)
    }
    /// Queue a row as inserted cells that save writes after the source rows,
    /// without loading the sheet model. Gives the values back when the sheet
    /// is loaded, has a committed model or has merged cells.
    fn append_streamed(
        &mut self,
        id: SheetId,
        values: Vec<CellValue>,
    ) -> Result<std::result::Result<RowIndex, Vec<CellValue>>> {
        let Some(index) = self.sheets.iter().position(|sheet| sheet.id == id) else {
            return Ok(Err(values));
        };
        let source = &self.sheets[index];
        // An empty row only moves the model's append position, so it needs the model.
        if values.is_empty()
            || source.loaded
            || source.original.is_none()
            || source.kind != crate::SheetKind::Worksheet
            || self.editor.model_is_dirty(&source.name)
        {
            return Ok(Err(values));
        }
        let dates = values.iter().any(|value| value.temporal_value().is_some());
        if dates {
            self.prepare_style_edit()?;
        }
        let scan = match self.sheets[index].append_scan {
            Some(scan) => scan,
            None => {
                let scan = self.scan_append_row(index)?;
                self.sheets[index].append_scan = Some(scan);
                scan
            }
        };
        let Some(extent) = scan else {
            return Ok(Err(values));
        };
        let name = self.sheets[index].name.clone();
        let row = RowIndex::new(extent.max(self.editor.pending_row_extent(&name)))?;
        let mut styles: Vec<(crabxl_core::DateKind, crabxl_core::StyleId)> = Vec::new();
        for value in &values {
            if let Some(date) = value.temporal_value()
                && !styles.iter().any(|(kind, _)| *kind == date.kind())
            {
                let style = self.bank.temporal_style(date.kind());
                if style.is_ok() {
                    self.editor.styles_changed();
                }
                styles.push((date.kind(), style?));
            }
        }
        self.editor.model_font_count = self.bank.style_catalog().map(|catalog| catalog.fonts.len());
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let scratch_allowance = maximum.saturating_sub(self.managed_retained_bytes());
        let plan = self
            .editor
            .prepare_row(&name, row, &values, scratch_allowance, true)?;
        let package = self
            .package_extra_bytes()
            .saturating_sub(self.editor.patch_bytes())
            .saturating_add(plan.bytes);
        let retained = self
            .mapping_bytes()
            .saturating_add(package)
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(plan.scratch_bytes);
        self.editor
            .book
            .rebalance_strings_for_retained(retained, maximum)?;
        let available = maximum
            .checked_sub(
                self.mapping_bytes()
                    .saturating_add(package)
                    .saturating_add(self.editor.book.retained_source_bytes())
                    .saturating_add(plan.scratch_bytes),
            )
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)?;
        self.editor.commit_row(plan, values, |value| {
            value
                .temporal_value()
                .and_then(|date| styles.iter().find(|(kind, _)| *kind == date.kind()))
                .map_or(crabxl_core::StyleId::new(0), |(_, style)| *style)
        });
        self.sheets[index].append_scan = Some(Some(row.get() + 1));
        self.rebalance()?;
        Ok(Ok(row))
    }
    /// Read the source rows once to find the append row without keeping cells.
    /// Returns None for sheets with merged cells, whose appends need the model.
    fn scan_append_row(&mut self, index: usize) -> Result<Option<u32>> {
        self.rebalance()?;
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let retained = self
            .bank
            .charged_bytes()
            .saturating_add(self.mapping_bytes())
            .saturating_add(self.package_extra_bytes());
        let name = self.sheets[index].name.clone();
        let (extent, merged) = {
            let mut rows = self.editor.book.rows_with_catalog_allowance(
                &name,
                self.options.read.clone(),
                Some(maximum),
                self.bank.style_catalog(),
                retained,
                true,
            )?;
            rows.capture_merges();
            rows.set_aggregate_retained(retained)?;
            let mut row = Row::new(RowIndex::new(0)?);
            let mut extent = 0;
            while rows.read_row_into(&mut row)? {
                extent = extent.max(row.index.get() + 1);
                row.cells.clear();
            }
            (extent, !rows.take_merge_ranges().is_empty())
        };
        self.rebalance()?;
        Ok((!merged).then_some(extent))
    }
    pub(super) fn edit_value(
        &mut self,
        id: SheetId,
        address: CellAddress,
        value: CellValue,
        insert_missing: bool,
    ) -> Result<()> {
        if value.temporal_value().is_some() {
            self.prepare_style_edit()?;
            self.sheet(id)?;
            if !insert_missing && self.bank.sheet(id)?.get(address).is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Replacement targets a missing model cell",
                )
                .with_cell(address));
            }
            self.edit_structure_when(
                id,
                |sheet| {
                    let style = sheet
                        .get(address)
                        .map_or(crabxl_core::StyleId::new(0), |cell| cell.style);
                    sheet.set(Cell {
                        address,
                        value,
                        style,
                    })
                },
                |_| true,
            )?;
            self.editor.styles_changed();
            return self.rebalance();
        }
        self.editor.model_font_count = self.bank.style_catalog().map(|catalog| catalog.fonts.len());
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
