//! Formula calculation before saving.
use super::LoadedWorkbook;
use crabxl_calc::{CalcStats, Calculation};
use crabxl_core::{Cell, CellAddress, CellValue, Result};
use std::{
    collections::HashMap,
    io::{Read, Seek},
};

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Calculate every formula and store each result as its cached value, so
    /// readers that do not calculate see current results. A formula without
    /// a result, such as an array formula, loses its old cached value.
    ///
    /// Call this right before saving: the next save keeps the new cached
    /// values, and edits made in between leave them out of date.
    /// Excel still recalculates the workbook when it opens it.
    pub fn calculate(&mut self) -> Result<CalcStats> {
        let mut stats = CalcStats::default();
        if self.options.read.data_only {
            return Ok(stats);
        }
        let mut calculation = Calculation::new(self.bank.epoch(), crate::max_threads());
        let mut formulas: HashMap<(usize, CellAddress), CellValue> = HashMap::new();
        let mut add = |calculation: &mut Calculation, position: usize, cell: &Cell| {
            if matches!(cell.value.unannotated(), CellValue::Formula(_)) {
                formulas.insert((position, cell.address), cell.value.clone());
            }
            calculation.add_cell(position, cell)
        };
        for index in 0..self.sheets.len() {
            let sheet = &self.sheets[index];
            let (id, name) = (sheet.id, sheet.name.clone());
            let position = calculation.add_sheet(&name)?;
            if sheet.kind != crate::SheetKind::Worksheet {
                continue;
            }
            if sheet.loaded || sheet.original.is_none() {
                for cell in self.bank.sheet(id)?.cells() {
                    add(&mut calculation, position, cell)?;
                }
            } else {
                // Pending edits replace the source cells at their addresses.
                let pending: HashMap<CellAddress, Cell> = self
                    .editor
                    .pending_cells(&name)
                    .map(|cell| (cell.address, cell.clone()))
                    .collect();
                let maximum = self
                    .allowance
                    .retained_data_bytes
                    .min(self.options.workbook.max_bytes);
                let retained = self
                    .bank
                    .charged_bytes()
                    .saturating_add(self.mapping_bytes())
                    .saturating_add(self.package_extra_bytes());
                let mut rows = self.editor.book.rows_with_catalog_allowance(
                    &name,
                    self.options.read.clone(),
                    Some(maximum),
                    self.bank.style_catalog(),
                    retained,
                    false,
                )?;
                while let Some(row) = rows.next_row()? {
                    for cell in &row.cells {
                        if !pending.contains_key(&cell.address) {
                            add(&mut calculation, position, cell)?;
                        }
                    }
                }
                drop(rows);
                for cell in pending.values() {
                    add(&mut calculation, position, cell)?;
                }
            }
            for table in &self.sheet_features(id)?.tables {
                calculation.add_table(position, table)?;
            }
        }
        for name in &self.workbook_features()?.defined_names {
            calculation.add_name(name);
        }
        let mut results = Vec::with_capacity(formulas.len());
        calculation.run(|position, address, value| {
            results.push((position, address, value));
            Ok(())
        })?;
        let ids: Vec<_> = self.sheets.iter().map(|sheet| sheet.id).collect();
        for (position, address, cached) in results {
            let Some(mut value) = formulas.remove(&(position, address)) else {
                continue;
            };
            let CellValue::Formula(formula) = value.unannotated_mut() else {
                continue;
            };
            if cached.is_some() {
                stats.calculated += 1;
            } else {
                stats.skipped += 1;
            }
            formula.set_cached(cached)?;
            self.edit_value(ids[position], address, value, false)?;
        }
        self.editor.keep_model_caches = true;
        Ok(stats)
    }
}
