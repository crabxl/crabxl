//! Apply a structural edit to the models that hold references.
//!
//! Every function here is pure: it returns an updated copy, or `None` when
//! nothing changes, and reports unsupported content as an error before the
//! caller mutates anything.

use super::{Axis, RangeUpdate, StructuralEdit};
use crate::drawing::{DrawingAnchor, DrawingEditAs, DrawingMarker, XmlContent, XmlElement};
use crate::features::{DefinedName, SheetFeatures};
use crate::{CellRange, CellValue, Error, ErrorKind, Formula, FormulaType, Result};

fn unsupported(message: String) -> Error {
    Error::new(ErrorKind::Unsupported, message)
}

impl StructuralEdit {
    /// Update a formula cell value held on `host`. Returns `None` when the
    /// value has no formula or its references do not change. Shared formulas
    /// become ordinary formulas with their expanded text. Array and data-table
    /// formulas that would change are rejected.
    pub fn update_value(
        &self,
        value: &CellValue,
        host: &str,
        sheet: &str,
    ) -> Result<Option<CellValue>> {
        match value {
            CellValue::Formula(formula) => Ok(self
                .update_formula_value(formula, host, sheet)?
                .map(|formula| CellValue::Formula(Box::new(formula)))),
            CellValue::Annotated(annotated) => {
                let Some(inner) = self.update_value(&annotated.value, host, sheet)? else {
                    return Ok(None);
                };
                let mut annotated = annotated.clone();
                annotated.value = inner;
                Ok(Some(CellValue::Annotated(annotated)))
            }
            _ => Ok(None),
        }
    }
    fn update_formula_value(
        &self,
        formula: &Formula,
        host: &str,
        sheet: &str,
    ) -> Result<Option<Formula>> {
        let Some(text) = self.update_formula(formula.expression(), Some(host), sheet)? else {
            return Ok(None);
        };
        match formula.formula_type() {
            FormulaType::Normal | FormulaType::Shared { .. } => {
                Formula::new(text, formula.cached().cloned()).map(Some)
            }
            _ => Err(unsupported(format!(
                "Cannot update the array or data-table formula {} on sheet '{host}'",
                formula.expression()
            ))),
        }
    }

    /// Update the ranges and formulas of the features of `sheet`, the edited
    /// sheet. Notes move with their cells elsewhere and are not changed here.
    pub fn update_sheet_features(
        &self,
        features: &SheetFeatures,
        sheet: &str,
    ) -> Result<Option<SheetFeatures>> {
        let mut updated = features.clone();
        let mut changed = false;
        if features.scenarios.is_some() {
            return Err(unsupported(format!(
                "Cannot update the scenarios of sheet '{sheet}' for a row or column edit"
            )));
        }
        for table in &mut updated.tables {
            changed |= self.update_table(table, sheet)?;
        }
        if let Some(filter) = &mut updated.auto_filter {
            changed |= self.update_filter(filter, sheet, "the auto filter")?;
        }
        if let Some(sort) = &mut updated.sort_state {
            changed |= self.update_sort(sort, sheet)?;
        }
        let mut formats = Vec::with_capacity(updated.conditional_formats.len());
        for mut format in std::mem::take(&mut updated.conditional_formats) {
            if let Some(ranges) = self.update_ranges(&format.ranges)? {
                changed = true;
                if ranges.is_empty() {
                    continue; // Every cell the rules applied to is deleted.
                }
                format.ranges = ranges;
            }
            for rule in &mut format.rules {
                for formula in &mut rule.formulas {
                    if let Some(text) = self.update_formula(formula, Some(sheet), sheet)? {
                        *formula = text.into();
                        changed = true;
                    }
                }
            }
            formats.push(format);
        }
        updated.conditional_formats = formats;
        let mut validations = Vec::with_capacity(updated.data_validations.validations.len());
        for mut validation in std::mem::take(&mut updated.data_validations.validations) {
            if let Some(ranges) = self.update_ranges(&validation.ranges)? {
                changed = true;
                if ranges.is_empty() {
                    continue;
                }
                validation.ranges = ranges;
            }
            for formula in [&mut validation.formula1, &mut validation.formula2]
                .into_iter()
                .flatten()
            {
                if let Some(text) = self.update_formula(formula, Some(sheet), sheet)? {
                    *formula = text.into();
                    changed = true;
                }
            }
            validations.push(validation);
        }
        updated.data_validations.validations = validations;
        Ok(changed.then_some(updated))
    }

    fn update_table(&self, table: &mut crate::features::Table, sheet: &str) -> Result<bool> {
        let name = table.display_name.clone();
        let range: CellRange = table.ref_range.parse()?;
        let (low, high) = match self.axis {
            Axis::Rows => (range.start.row.get(), range.end.row.get()),
            Axis::Columns => (range.start.column.get(), range.end.column.get()),
        };
        let touches = |index: u32| (self.at..self.at + self.count).contains(&index);
        if self.axis == Axis::Columns {
            let inside = if self.insert {
                self.at > low && self.at <= high
            } else {
                (low..=high).any(touches)
            };
            if inside {
                return Err(unsupported(format!(
                    "Cannot insert or delete columns inside table '{name}'"
                )));
            }
        } else if !self.insert {
            let header = table.header_row_count.unwrap_or(1) > 0 && touches(low);
            let totals = table.totals_row_count.unwrap_or(0) > 0 && touches(high);
            if header || totals {
                return Err(unsupported(format!(
                    "Cannot delete the header or totals row of table '{name}'"
                )));
            }
        }
        let mut changed = false;
        match self.update_range(range)? {
            RangeUpdate::Unchanged => {}
            RangeUpdate::Moved(moved) => {
                table.ref_range = moved.to_string().into();
                changed = true;
            }
            RangeUpdate::Removed => {
                return Err(unsupported(format!(
                    "Cannot delete every row of table '{name}'"
                )));
            }
        }
        if let Some(filter) = &mut table.auto_filter {
            changed |= self.update_filter(filter, sheet, "a table filter")?;
        }
        if let Some(sort) = &mut table.sort_state {
            changed |= self.update_sort(sort, sheet)?;
        }
        for column in &mut table.table_columns {
            for formula in [
                &mut column.calculated_column_formula,
                &mut column.totals_row_formula,
            ]
            .into_iter()
            .flatten()
            {
                if let Some(text) = self.update_formula(&formula.text, Some(sheet), sheet)? {
                    formula.text = text.into();
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    fn update_filter(
        &self,
        filter: &mut crate::features::AutoFilter,
        sheet: &str,
        what: &str,
    ) -> Result<bool> {
        let mut changed = false;
        if let Some(text) = &filter.ref_range {
            let range: CellRange = text.parse()?;
            if self.axis == Axis::Columns && !filter.filter_column.is_empty() {
                let (low, high) = (range.start.column.get(), range.end.column.get());
                let shifts_columns = if self.insert {
                    self.at > low && self.at <= high
                } else {
                    self.at <= high && self.at + self.count > low
                };
                if shifts_columns {
                    return Err(unsupported(format!(
                        "Cannot insert or delete filtered columns inside {what}"
                    )));
                }
            }
            match self.update_range(range)? {
                RangeUpdate::Unchanged => {}
                RangeUpdate::Moved(moved) => {
                    filter.ref_range = Some(moved.to_string().into());
                    changed = true;
                }
                RangeUpdate::Removed => {
                    return Err(unsupported(format!("Cannot delete every cell of {what}")));
                }
            }
        }
        if let Some(sort) = &mut filter.sort_state {
            changed |= self.update_sort(sort, sheet)?;
        }
        Ok(changed)
    }

    fn update_sort(&self, sort: &mut crate::features::SortState, sheet: &str) -> Result<bool> {
        let range: CellRange = sort.ref_range.parse()?;
        let mut changed = false;
        match self.update_range(range)? {
            RangeUpdate::Unchanged => {}
            RangeUpdate::Moved(moved) => {
                sort.ref_range = moved.to_string().into();
                changed = true;
            }
            RangeUpdate::Removed => {
                return Err(unsupported(format!(
                    "Cannot delete every cell of a sort range on sheet '{sheet}'"
                )));
            }
        }
        for condition in &mut sort.sort_condition {
            let range: CellRange = condition.ref_range.parse()?;
            match self.update_range(range)? {
                RangeUpdate::Unchanged => {}
                RangeUpdate::Moved(moved) => {
                    condition.ref_range = moved.to_string().into();
                    changed = true;
                }
                RangeUpdate::Removed => {
                    return Err(unsupported(format!(
                        "Cannot delete a sorted column on sheet '{sheet}'"
                    )));
                }
            }
        }
        Ok(changed)
    }

    /// Move a drawing anchor on the edited sheet. Two-cell anchors that move
    /// and resize with cells grow and shrink like ranges. Objects anchored
    /// completely in deleted rows or columns are rejected.
    pub fn update_anchor(&self, anchor: &DrawingAnchor) -> Result<Option<DrawingAnchor>> {
        let index = |marker: &DrawingMarker| match self.axis {
            Axis::Rows => marker.row,
            Axis::Columns => marker.col,
        };
        let place = |marker: &DrawingMarker, value: u32, reset: bool| {
            let mut marker = *marker;
            match self.axis {
                Axis::Rows => {
                    marker.row = value;
                    if reset {
                        marker.row_off = 0;
                    }
                }
                Axis::Columns => {
                    marker.col = value;
                    if reset {
                        marker.col_off = 0;
                    }
                }
            }
            marker
        };
        let deleted = || unsupported("Cannot delete the cells that anchor a drawing object".into());
        match anchor {
            DrawingAnchor::Absolute { .. } => Ok(None),
            DrawingAnchor::OneCell { from, extent } => match self.map_index(index(from))? {
                None => Err(deleted()),
                Some(value) if value == index(from) => Ok(None),
                Some(value) => Ok(Some(DrawingAnchor::OneCell {
                    from: place(from, value, false),
                    extent: *extent,
                })),
            },
            DrawingAnchor::TwoCell { from, to, edit_as } => {
                let (start, end) = (index(from), index(to));
                let (new_start, new_end) = match edit_as {
                    Some(DrawingEditAs::Absolute) => return Ok(None),
                    Some(DrawingEditAs::OneCell) => {
                        // Moves with its first cell; keeps its size.
                        let Some(value) = self.map_index(start)? else {
                            return Err(deleted());
                        };
                        let shift = i64::from(value) - i64::from(start);
                        let end = u32::try_from(i64::from(end) + shift).map_err(|_| deleted())?;
                        (value, end)
                    }
                    _ => match self.map_span(start, end)? {
                        None => return Err(deleted()),
                        Some(span) => span,
                    },
                };
                if (new_start, new_end) == (start, end) {
                    return Ok(None);
                }
                let reset_from = !self.insert && start >= self.at && start < self.at + self.count;
                let reset_to = !self.insert && end >= self.at && end < self.at + self.count;
                Ok(Some(DrawingAnchor::TwoCell {
                    from: place(from, new_start, reset_from),
                    to: place(to, new_end, reset_to),
                    edit_as: *edit_as,
                }))
            }
        }
    }

    /// Update the series and label formulas (`c:f`) of a chart part.
    pub fn update_chart(&self, space: &XmlElement, sheet: &str) -> Result<Option<XmlElement>> {
        let mut updated = space.clone();
        let changed = self.update_chart_element(&mut updated, sheet)?;
        Ok(changed.then_some(updated))
    }
    fn update_chart_element(&self, element: &mut XmlElement, sheet: &str) -> Result<bool> {
        if element.local_name() == "extLst" {
            return Ok(false);
        }
        let mut changed = false;
        if element.local_name() == "f"
            && let [XmlContent::Text(text)] = element.children.as_mut_slice()
        {
            if let Some(updated) = self.update_formula(text, None, sheet)? {
                *text = updated.into();
                return Ok(true);
            }
            return Ok(false);
        }
        for child in &mut element.children {
            if let XmlContent::Element(child) = child {
                changed |= self.update_chart_element(child, sheet)?;
            }
        }
        Ok(changed)
    }

    /// Update defined names, including print areas and print titles.
    pub fn update_defined_names(
        &self,
        names: &[DefinedName],
        sheet: &str,
    ) -> Result<Option<Vec<DefinedName>>> {
        let mut updated = names.to_vec();
        let mut changed = false;
        for name in &mut updated {
            let text = self
                .update_formula(&name.value, None, sheet)
                .map_err(|error| {
                    if error.kind() == ErrorKind::Unsupported {
                        unsupported(format!(
                            "Cannot update the defined name '{}': {}",
                            name.name,
                            error.message()
                        ))
                    } else {
                        error
                    }
                })?;
            if let Some(text) = text {
                name.value = text.into();
                changed = true;
            }
        }
        Ok(changed.then_some(updated))
    }

    /// Reject pivot tables that read from or sit on the edited sheet: their
    /// caches and layouts cannot be updated without recalculation.
    pub fn check_pivots(
        &self,
        pivots: &[crate::features::PivotTable],
        host: &str,
        sheet: &str,
    ) -> Result<()> {
        for pivot in pivots {
            if host.eq_ignore_ascii_case(sheet) {
                return Err(unsupported(format!(
                    "Cannot insert or delete rows or columns on sheet '{sheet}', which holds a pivot table"
                )));
            }
            if reads_sheet(&pivot.cache.definition, sheet) {
                return Err(unsupported(format!(
                    "Cannot insert or delete rows or columns on sheet '{sheet}', the source of a pivot table on sheet '{host}'"
                )));
            }
        }
        Ok(())
    }
}

fn reads_sheet(element: &XmlElement, sheet: &str) -> bool {
    if element.local_name() == "worksheetSource"
        && element
            .attribute("sheet")
            .is_some_and(|source| source.eq_ignore_ascii_case(sheet))
    {
        return true;
    }
    element.children.iter().any(|child| match child {
        XmlContent::Element(child) => reads_sheet(child, sheet),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::features::{ConditionalFormatting, DataValidation, Table};

    #[test]
    fn features_follow_row_insertion() {
        let edit = StructuralEdit::insert_rows(2, 3).unwrap();
        let mut features = SheetFeatures::default();
        let table = Table {
            display_name: "Sales".into(),
            ref_range: "A1:C10".into(),
            ..Default::default()
        };
        features.tables.push(table);
        features.conditional_formats.push(ConditionalFormatting {
            ranges: vec!["B2:B10".parse().unwrap()],
            ..Default::default()
        });
        let validation = DataValidation {
            ranges: vec!["D5".parse().unwrap()],
            formula1: Some("$E$5:$E$9".into()),
            ..Default::default()
        };
        features.data_validations.validations.push(validation);
        let updated = edit
            .update_sheet_features(&features, "Data")
            .unwrap()
            .unwrap();
        assert_eq!(&*updated.tables[0].ref_range, "A1:C13");
        assert_eq!(
            updated.conditional_formats[0].ranges[0].to_string(),
            "B2:B13"
        );
        assert_eq!(
            updated.data_validations.validations[0].ranges[0].to_string(),
            "D8"
        );
        assert_eq!(
            updated.data_validations.validations[0].formula1.as_deref(),
            Some("$E$8:$E$12")
        );
        let delete_header = StructuralEdit::delete_rows(0, 1).unwrap();
        assert_eq!(
            delete_header
                .update_sheet_features(&features, "Data")
                .unwrap_err()
                .kind(),
            ErrorKind::Unsupported
        );
        let inside = StructuralEdit::insert_columns(1, 1).unwrap();
        assert!(inside.update_sheet_features(&features, "Data").is_err());
    }

    #[test]
    fn anchors_and_names_follow_edits() {
        let marker = |row, col| DrawingMarker::new(row, col).unwrap();
        let anchor = DrawingAnchor::TwoCell {
            from: marker(4, 1),
            to: marker(9, 3),
            edit_as: None,
        };
        let insert = StructuralEdit::insert_rows(6, 2).unwrap();
        let DrawingAnchor::TwoCell { from, to, .. } =
            insert.update_anchor(&anchor).unwrap().unwrap()
        else {
            panic!()
        };
        assert_eq!((from.row, to.row), (4, 11));
        let delete = StructuralEdit::delete_rows(3, 8).unwrap();
        assert!(delete.update_anchor(&anchor).is_err());
        let names = vec![DefinedName {
            name: "_xlnm.Print_Area".into(),
            value: "Data!$A$1:$C$20".into(),
            ..Default::default()
        }];
        let updated = insert
            .update_defined_names(&names, "Data")
            .unwrap()
            .unwrap();
        assert_eq!(&*updated[0].value, "Data!$A$1:$C$22");
    }
}
