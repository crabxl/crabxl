//! Structure operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Remove one physical cell from a supported source-backed model and
    /// transfer its owned value/style to the caller. Logical append extent stays.
    /// Affected unmodeled graphs reject before any cell or package mutation.
    pub fn remove_cell(&mut self, id: SheetId, address: CellAddress) -> Result<Option<Cell>> {
        self.remove_cell_with_hyperlink(id, address)
            .map(|(cell, _)| cell)
    }
    /// Remove a point and transfer its metadata after the original graph preflight.
    pub fn remove_cell_with_hyperlink(
        &mut self,
        id: SheetId,
        address: CellAddress,
    ) -> Result<(Option<Cell>, Option<crabxl_core::Hyperlink>)> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let adopted = self.sheets[index].features.is_none();
        let result = (|| {
            let mut comments = self.sheet_features(id)?.comments.clone();
            comments.retain(|comment| comment.address != address);
            let result = self
                .edit_structure_when(
                    id,
                    |sheet| {
                        let removed = sheet.remove_with_hyperlink(address)?;
                        let changed = removed.0.is_some() || removed.1.is_some();
                        Ok((removed, changed))
                    },
                    |(_, changed)| *changed,
                )
                .map(|(cell, _)| cell)?;
            if let Some(source) = self.sheets.iter_mut().find(|source| source.id == id)
                && let Some(state) = &mut source.features
            {
                state.current.comments = comments;
            }
            Ok(result)
        })();
        if result.is_err() && adopted {
            self.sheets[index].features = None;
            self.rebalance()?;
        }
        result
    }
    /// Insert `count` rows before row `at` and update every reference to the
    /// moved cells. See [`Self::insert_rows_with`].
    pub fn insert_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.insert_rows_with(id, at, count, ReferenceUpdate::All)
    }
    /// Delete `count` rows starting at `at` and update every reference.
    pub fn delete_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.delete_rows_with(id, at, count, ReferenceUpdate::All)
    }
    /// Insert `count` columns before column `at` and update every reference.
    pub fn insert_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.insert_columns_with(id, at, count, ReferenceUpdate::All)
    }
    /// Delete `count` columns starting at `at` and update every reference.
    pub fn delete_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.delete_columns_with(id, at, count, ReferenceUpdate::All)
    }
    /// Insert rows. With [`ReferenceUpdate::All`], formulas on every sheet,
    /// defined names, tables, rules, validations, filters, merges, row
    /// heights, chart series and drawing anchors follow the moved cells.
    /// Content that cannot follow (pivot tables, scenarios, 3-D references,
    /// array formulas) fails with `ErrorKind::Unsupported` before any change.
    pub fn insert_rows_with(
        &mut self,
        id: SheetId,
        at: RowIndex,
        count: u32,
        references: ReferenceUpdate,
    ) -> Result<()> {
        let edit = crabxl_core::StructuralEdit::insert_rows(at.get(), count)?;
        self.structural_edit(id, edit, references)
    }
    /// Delete rows. See [`Self::insert_rows_with`].
    pub fn delete_rows_with(
        &mut self,
        id: SheetId,
        at: RowIndex,
        count: u32,
        references: ReferenceUpdate,
    ) -> Result<()> {
        let edit = crabxl_core::StructuralEdit::delete_rows(at.get(), count)?;
        self.structural_edit(id, edit, references)
    }
    /// Insert columns. See [`Self::insert_rows_with`].
    pub fn insert_columns_with(
        &mut self,
        id: SheetId,
        at: ColumnIndex,
        count: u32,
        references: ReferenceUpdate,
    ) -> Result<()> {
        let edit = crabxl_core::StructuralEdit::insert_columns(at.get(), count)?;
        self.structural_edit(id, edit, references)
    }
    /// Delete columns. See [`Self::insert_rows_with`].
    pub fn delete_columns_with(
        &mut self,
        id: SheetId,
        at: ColumnIndex,
        count: u32,
        references: ReferenceUpdate,
    ) -> Result<()> {
        let edit = crabxl_core::StructuralEdit::delete_columns(at.get(), count)?;
        self.structural_edit(id, edit, references)
    }
    fn structural_edit(
        &mut self,
        id: SheetId,
        edit: crabxl_core::StructuralEdit,
        references: ReferenceUpdate,
    ) -> Result<()> {
        if references == ReferenceUpdate::All {
            return self.structural_edit_with_references(id, edit);
        }
        self.shift_cells(id, edit)
    }
    /// Move cells and notes only; references keep their text.
    fn shift_cells(&mut self, id: SheetId, edit: crabxl_core::StructuralEdit) -> Result<()> {
        let rows = edit.axis == crabxl_core::Axis::Rows;
        let (at, count, insert) = (edit.at, edit.count, edit.insert);
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Shift {
                at,
                count,
                rows,
                insert,
            },
            |sheet| match (rows, insert) {
                (true, true) => sheet.insert_rows(RowIndex::new(at)?, count),
                (true, false) => sheet.delete_rows(RowIndex::new(at)?, count),
                (false, true) => sheet.insert_columns(ColumnIndex::new(at)?, count),
                (false, false) => sheet.delete_columns(ColumnIndex::new(at)?, count),
            },
        )
    }
    fn structural_edit_with_references(
        &mut self,
        id: SheetId,
        edit: crabxl_core::StructuralEdit,
    ) -> Result<()> {
        let plan = self.plan_reference_updates(id, &edit)?;
        self.shift_cells(id, edit)?;
        self.apply_reference_updates(id, &edit, plan)
    }
    /// Compute every update before any change, so unsupported content
    /// rejects the edit while the workbook is still untouched.
    fn plan_reference_updates(
        &mut self,
        id: SheetId,
        edit: &crabxl_core::StructuralEdit,
    ) -> Result<ReferencePlan> {
        let sheet = self.bank.sheet(id)?.name().to_owned();
        let worksheets: Vec<SheetId> = self
            .sheets
            .iter()
            .filter(|entry| entry.kind == crate::SheetKind::Worksheet)
            .map(|entry| entry.id)
            .collect();
        let mut plan = ReferencePlan::default();
        let chartsheets: Vec<SheetId> = self
            .sheets
            .iter()
            .filter(|entry| entry.kind == crate::SheetKind::ChartSheet)
            .map(|entry| entry.id)
            .collect();
        for other in chartsheets {
            let mut charts = self.sheet_charts(other)?.to_vec();
            let mut changed = false;
            for chart in &mut charts {
                if let Some(space) = edit.update_chart(&chart.space, &sheet)? {
                    chart.space = space;
                    changed = true;
                }
            }
            if changed {
                plan.charts.push((other, charts));
            }
        }
        for &other in &worksheets {
            let host = self.bank.sheet(other)?.name().to_owned();
            let pivots = self.sheet_pivots(other)?.to_vec();
            edit.check_pivots(&pivots, &host, &sheet)?;
            let charts = self.sheet_charts(other)?.to_vec();
            let mut updated_charts = charts.clone();
            let mut charts_changed = false;
            for chart in &mut updated_charts {
                if let Some(space) = edit.update_chart(&chart.space, &sheet)? {
                    chart.space = space;
                    charts_changed = true;
                }
                if other == id
                    && let Some(anchor) = edit.update_anchor(&chart.anchor)?
                {
                    chart.anchor = anchor;
                    charts_changed = true;
                }
            }
            if charts_changed {
                plan.charts.push((other, updated_charts));
            }
            let mut links = Vec::new();
            for (address, link) in self.hyperlinks(other)?.iter() {
                let Some(location) = link.location.as_deref() else {
                    continue;
                };
                // A location is a reference or a defined name, without "=".
                if let Some(updated) = edit.update_formula(location, Some(&host), &sheet)? {
                    let mut link = link.clone();
                    link.location = Some(updated.into());
                    links.push((address, link));
                }
            }
            if !links.is_empty() {
                plan.links.push((other, links));
            }
            let mut formulas = Vec::new();
            for cell in self.sheet(other)?.cells() {
                if let Some(value) = edit.update_value(&cell.value, &host, &sheet)? {
                    formulas.push((cell.address, value));
                }
            }
            if !formulas.is_empty() {
                plan.formulas.push((other, formulas));
            }
        }
        plan.features = edit.update_sheet_features(self.sheet_features(id)?, &sheet)?;
        let mut images = self.sheet_images(id)?.to_vec();
        let mut images_changed = false;
        for image in &mut images {
            if let Some(anchor) = edit.update_anchor(&image.anchor)? {
                image.anchor = anchor;
                images_changed = true;
            }
        }
        plan.images = images_changed.then_some(images);
        let mut shapes = self.sheet_shapes(id)?.to_vec();
        let mut shapes_changed = false;
        for shape in &mut shapes {
            if let Some(anchor) = edit.update_anchor(&shape.anchor)? {
                shape.anchor = anchor;
                shapes_changed = true;
            }
        }
        plan.shapes = shapes_changed.then_some(shapes);
        let names = &self.workbook_features()?.defined_names;
        plan.names = edit.update_defined_names(names, &sheet)?;
        let model = self.sheet(id)?;
        let merges: Vec<CellRange> = model
            .merged_ranges()
            .ranges()
            .iter()
            .map(|merge| merge.range())
            .collect();
        plan.merges = edit.update_ranges(&merges)?;
        plan.dimensions = updated_dimensions(model.dimensions(), edit)?;
        Ok(plan)
    }
    fn apply_reference_updates(
        &mut self,
        id: SheetId,
        edit: &crabxl_core::StructuralEdit,
        plan: ReferencePlan,
    ) -> Result<()> {
        for (sheet, formulas) in plan.formulas {
            for (address, value) in formulas {
                let address = if sheet == id {
                    match moved(edit, address)? {
                        Some(address) => address,
                        None => continue, // The cell itself was deleted.
                    }
                } else {
                    address
                };
                self.set_value(sheet, address, value)?;
            }
        }
        for (sheet, links) in plan.links {
            for (address, link) in links {
                let address = if sheet == id {
                    match moved(edit, address)? {
                        Some(address) => address,
                        None => continue,
                    }
                } else {
                    address
                };
                self.update_hyperlink(sheet, address, Some(link))?;
            }
        }
        for (sheet, charts) in plan.charts {
            self.set_sheet_charts(sheet, charts)?;
        }
        if let Some(features) = plan.features {
            let mut features = features;
            // Notes moved with their cells during the shift.
            features.comments = self.sheet_features(id)?.comments.clone();
            self.set_sheet_features(id, features)?;
        }
        if let Some(images) = plan.images {
            self.set_sheet_images(id, images)?;
        }
        if let Some(shapes) = plan.shapes {
            self.set_sheet_shapes(id, shapes)?;
        }
        if let Some(names) = plan.names {
            let mut features = self.workbook_features()?.clone();
            features.defined_names = names;
            self.set_workbook_features(features)?;
        }
        if let Some(merges) = plan.merges {
            self.replace_merge_declarations(id, &merges)?;
        }
        if let Some(dimensions) = plan.dimensions {
            self.bank.sheet_mut(id)?.set_dimensions(dimensions)?;
        }
        Ok(())
    }
    /// Move a source-backed rectangle; reference expressions remain unchanged.
    pub fn move_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Range {
                range,
                rows,
                columns,
                copy: false,
            },
            |sheet| sheet.move_range(range, rows, columns),
        )
    }
    /// Move and translate relative references inside moved normal formulas.
    pub fn move_range_translated(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Range {
                range,
                rows,
                columns,
                copy: false,
            },
            |sheet| sheet.move_range_translated(range, rows, columns),
        )
    }
    /// Copy an actual rectangle under aggregate cell/payload allowances.
    pub fn copy_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure_with_comments(
            id,
            crabxl_core::features::CommentEdit::Range {
                range,
                rows,
                columns,
                copy: true,
            },
            |sheet| sheet.copy_range(range, rows, columns),
        )
    }
    fn edit_structure_with_comments(
        &mut self,
        id: SheetId,
        edit_comments: crabxl_core::features::CommentEdit,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<()>,
    ) -> Result<()> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let adopted = self.sheets[index].features.is_none();
        let staged = (|| {
            let comments = edit_comments.apply(&self.sheet_features(id)?.comments)?;
            let scratch = comments
                .iter()
                .map(|comment| size_of::<crabxl_core::features::Comment>() + comment.heap_bytes())
                .sum();
            self.edit_structure_with_work(id, edit, |_| true, scratch)?;
            Ok::<_, Error>(comments)
        })();
        let comments = match staged {
            Ok(comments) => comments,
            Err(error) => {
                if adopted {
                    self.sheets[index].features = None;
                    self.rebalance()?;
                }
                return Err(error);
            }
        };
        if let Some(source) = self.sheets.iter_mut().find(|source| source.id == id)
            && let Some(state) = &mut source.features
        {
            state.current.comments = comments;
        }
        Ok(())
    }
    pub(super) fn edit_structure(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<()>,
    ) -> Result<()> {
        self.edit_structure_when(id, edit, |_| true)
    }
    pub(super) fn edit_structure_when<T>(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<T>,
        changed: impl FnOnce(&T) -> bool,
    ) -> Result<T> {
        self.edit_structure_with_work(id, edit, changed, 0)
    }
    fn edit_structure_with_work<T>(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<T>,
        changed: impl FnOnce(&T) -> bool,
        scratch: usize,
    ) -> Result<T> {
        let index = self
            .sheets
            .iter()
            .position(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].kind != crate::SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Chartsheets do not contain editable cells",
            ));
        }
        if self.sheets[index].original.is_none() {
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
            self.reserve_workbook_patch(self.editor.patch_bytes().saturating_add(scratch))?;
            let result = edit(&mut self.bank.sheet_mut(id)?);
            if let Ok(value) = &result
                && changed(value)
            {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let original = self.sheets[index].original.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidState,
                "Missing structural source identity",
            )
        })?;
        // A committed model was validated when it was committed, and later edits
        // go through checked model APIs, so a full pass per edit is not needed.
        let validated = self.editor.model_is_dirty(&self.sheets[index].name);
        let mut plan = self.editor.prepare_model(&self.sheets[index].name)?;
        self.hyperlinks(id)?;
        let had_hyperlinks = !self.bank.sheet(id)?.hyperlinks().is_empty();
        let hyperlink_bytes = if had_hyperlinks {
            Some(self.editor.prepare_hyperlink_patch(original, plan.bytes)?)
        } else {
            None
        };
        if let Some(bytes) = hyperlink_bytes {
            plan.bytes = bytes;
        }
        if !validated {
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
        }
        self.validate_normalized_styles(id)?;
        self.reserve_workbook_patch(
            plan.bytes
                .max(self.editor.patch_bytes())
                .saturating_add(scratch),
        )?;
        let result = edit(&mut self.bank.sheet_mut(id)?);
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                self.rebalance()?;
                return Err(error);
            }
        };
        if changed(&value) {
            self.commit_normalized_model(plan, id);
            if let Some(bytes) = hyperlink_bytes {
                self.editor.commit_hyperlink_patch(original, id, bytes);
            }
        }
        self.rebalance()?;
        Ok(value)
    }
}

/// Whether a row or column edit updates references to the moved cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReferenceUpdate {
    /// Update formulas, names, ranges and anchors that refer to moved cells.
    #[default]
    All,
    /// Move cells and notes only. References keep their text.
    CellsOnly,
}

/// Where a cell of the edited sheet moved; `None` when it was deleted.
fn moved(edit: &crabxl_core::StructuralEdit, address: CellAddress) -> Result<Option<CellAddress>> {
    let (row, column) = match edit.axis {
        crabxl_core::Axis::Rows => (
            edit.map_index(address.row.get())?,
            Some(address.column.get()),
        ),
        crabxl_core::Axis::Columns => (
            Some(address.row.get()),
            edit.map_index(address.column.get())?,
        ),
    };
    Ok(match (row, column) {
        (Some(row), Some(column)) => Some(CellAddress::new(row, column)?),
        _ => None,
    })
}

#[derive(Default)]
struct ReferencePlan {
    formulas: Vec<(SheetId, Vec<(CellAddress, CellValue)>)>,
    links: Vec<(SheetId, Vec<(CellAddress, crabxl_core::Hyperlink)>)>,
    charts: Vec<(SheetId, Vec<crabxl_core::drawing::Chart>)>,
    features: Option<crabxl_core::SheetFeatures>,
    images: Option<Vec<crabxl_core::Image>>,
    shapes: Option<Vec<crabxl_core::drawing::Shape>>,
    names: Option<Vec<crabxl_core::features::DefinedName>>,
    merges: Option<Vec<CellRange>>,
    dimensions: Option<crabxl_core::SheetDimensions>,
}

/// Row heights and column widths follow their rows and columns.
fn updated_dimensions(
    dimensions: &crabxl_core::SheetDimensions,
    edit: &crabxl_core::StructuralEdit,
) -> Result<Option<crabxl_core::SheetDimensions>> {
    let mut updated = dimensions.clone();
    let mut changed = false;
    match edit.axis {
        crabxl_core::Axis::Rows => {
            for row in dimensions.rows() {
                let moved = edit.map_index(row.index.get())?;
                if moved == Some(row.index.get()) {
                    continue;
                }
                changed = true;
                updated.remove_row(row.index);
            }
            for row in dimensions.rows() {
                if let Some(index) = edit.map_index(row.index.get())?
                    && index != row.index.get()
                {
                    let mut row = row.clone();
                    row.index = RowIndex::new(index)?;
                    updated.set_row(row, usize::MAX)?;
                }
            }
        }
        crabxl_core::Axis::Columns => {
            for column in dimensions.columns() {
                let span = edit.map_span(column.start.get(), column.end.get())?;
                if span == Some((column.start.get(), column.end.get())) {
                    continue;
                }
                changed = true;
                updated.remove_column(column.start);
            }
            for column in dimensions.columns() {
                if let Some((start, end)) = edit.map_span(column.start.get(), column.end.get())?
                    && (start, end) != (column.start.get(), column.end.get())
                {
                    let mut column = column.clone();
                    column.start = ColumnIndex::new(start)?;
                    column.end = ColumnIndex::new(end)?;
                    updated.set_column(column, usize::MAX)?;
                }
            }
        }
    }
    Ok(changed.then_some(updated))
}
