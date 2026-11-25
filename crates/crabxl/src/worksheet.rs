//! A worksheet in an editable workbook.

use crate::features::{
    Chart, ConditionalFormat, DataValidation, Hyperlink, Image, Margins, Note, Orientation,
    PageSetup, PlacedImage, Table,
};
use crate::reference::{CellRef, IntoCellRef, IntoRange, IntoSpan, RangeRef, Span};
use crate::row::{FromRow, Grid, IntoRow};
use crate::style::{Color, Style};
use crate::value::{CellInput, Formula, IntoCell, Value, from_core, to_core};
use crate::workbook::{Visibility, Workbook};
use crate::{Error, ErrorKind, Result};
use crabxl_core::{CellAddress, ColumnDimension, ColumnIndex, RowDimension, RowIndex, SheetId};

/// One cell: its value, formula and style.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Cell {
    /// The position.
    pub at: CellRef,
    /// The value, or the cached result of the formula.
    pub value: Value,
    /// The formula, if the cell has one.
    pub formula: Option<Formula>,
    /// The appearance.
    pub style: Style,
}

/// A worksheet. It borrows its workbook; use one worksheet at a time.
///
/// Reading loads the sheet on first use. Reading never changes the sheet:
/// missing cells read as [`Value::Blank`] without being created.
pub struct Worksheet<'a> {
    book: &'a mut Workbook,
    id: SheetId,
}

impl<'a> Worksheet<'a> {
    pub(crate) fn new(book: &'a mut Workbook, id: SheetId) -> Self {
        Self { book, id }
    }
    fn context(&self, error: Error) -> Error {
        error.with_sheet(self.book.sheet_name(self.id))
    }
    fn model(&mut self) -> Result<&crabxl_core::Worksheet> {
        let id = self.id;
        self.book.engine.sheet(id)
    }

    /// The sheet name.
    pub fn name(&self) -> String {
        self.book.sheet_name(self.id)
    }
    /// Rename the sheet. Formulas that refer to it keep their text.
    pub fn rename(&mut self, name: &str) -> Result<()> {
        self.book.engine.rename_sheet(self.id, name)
    }
    /// The visibility of the sheet tab.
    pub fn visibility(&self) -> Result<Visibility> {
        Ok(self.book.engine.model().sheet(self.id)?.visibility())
    }
    /// Show or hide the sheet tab.
    pub fn set_visibility(&mut self, visibility: Visibility) -> Result<()> {
        self.book.engine.set_sheet_visibility(self.id, visibility)
    }

    // ----- Reading -----

    /// One cell: value, formula and style.
    pub fn cell(&mut self, at: impl IntoCellRef) -> Result<Cell> {
        let at = at.into_cell_ref()?;
        let (value, style_id) = match self.model()?.get(at.0) {
            Some(cell) => (cell.value.clone(), cell.style),
            None => (crabxl_core::CellValue::Empty, self.model()?.style_at(at.0)),
        };
        let (value, formula) = from_core(value);
        let style = Style::from_core(self.book.engine.cell_style(style_id)?);
        Ok(Cell {
            at,
            value,
            formula,
            style,
        })
    }
    /// The value of a cell; for a formula cell, its cached result.
    pub fn value(&mut self, at: impl IntoCellRef) -> Result<Value> {
        let at = at.into_cell_ref()?;
        Ok(match self.model()?.get(at.0) {
            Some(cell) => from_core(cell.value.clone()).0,
            None => Value::Blank,
        })
    }
    /// The formula of a cell, if it has one.
    pub fn formula(&mut self, at: impl IntoCellRef) -> Result<Option<Formula>> {
        let at = at.into_cell_ref()?;
        Ok(match self.model()?.get(at.0) {
            Some(cell) => from_core(cell.value.clone()).1,
            None => None,
        })
    }
    /// The style of a cell.
    pub fn style(&mut self, at: impl IntoCellRef) -> Result<Style> {
        let at = at.into_cell_ref()?;
        let id = self.model()?.style_at(at.0);
        self.book.engine.cell_style(id).map(Style::from_core)
    }
    /// The values of a range as a dense grid.
    pub fn values(&mut self, range: impl IntoRange) -> Result<Grid> {
        let range = range.into_range()?;
        let (top, left) = (range.start().row(), range.start().column());
        let mut grid = Grid::with_size(range.width() as usize, range.height() as usize);
        let model = self.model()?;
        for row in top..=range.end().row() {
            for cell in model.row_cells_from(RowIndex::new(row)?, ColumnIndex::new(left)?) {
                let column = cell.address.column.get();
                if column > range.end().column() {
                    break;
                }
                grid.set(
                    (row - top) as usize,
                    (column - left) as usize,
                    from_core(cell.value.clone()).0,
                );
            }
        }
        Ok(grid)
    }
    /// The rows of a range, each converted to `T`, such as `(String, f64)`.
    pub fn rows_as<T: FromRow>(&mut self, range: impl IntoRange) -> Result<Vec<T>> {
        let grid = self.values(range)?;
        grid.rows_as().map_err(|error| self.context(error))
    }
    /// The smallest range that holds every present cell, or `None` for an
    /// empty sheet.
    pub fn used_range(&mut self) -> Result<Option<RangeRef>> {
        let model = self.model()?;
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for cell in model.cells() {
            let (row, column) = (cell.address.row.get(), cell.address.column.get());
            bounds = Some(match bounds {
                None => (row, column, row, column),
                Some((top, left, bottom, right)) => (
                    top.min(row),
                    left.min(column),
                    bottom.max(row),
                    right.max(column),
                ),
            });
        }
        bounds
            .map(|(top, left, bottom, right)| {
                RangeRef::new(CellRef::new(top, left)?, CellRef::new(bottom, right)?)
            })
            .transpose()
    }

    // ----- Writing -----

    /// Write a value or a [`Formula`] to a cell. The cell keeps its style.
    pub fn set_value(&mut self, at: impl IntoCellRef, value: impl IntoCell) -> Result<()> {
        let at = at.into_cell_ref()?;
        // The engine gives a date or time in a cell without a date format the
        // default format of its kind, so it reads back as the same kind.
        let value = to_core(value.into_cell()).map_err(|error| error.with_cell(at.0))?;
        let id = self.id;
        self.book
            .engine
            .upsert_value(id, at.0, value)
            .map_err(|error| self.context(error))?;
        Ok(())
    }
    /// Write a formula to a cell, with or without the leading `=`.
    pub fn set_formula(&mut self, at: impl IntoCellRef, formula: &str) -> Result<()> {
        self.set_value(at, Formula::new(formula)?)
    }
    /// Write rows of values starting at the top-left cell of `range`. For a
    /// one-cell range, the rows may have any size; for a larger range, the
    /// rows must match its size exactly.
    pub fn set_values<R: IntoRow>(
        &mut self,
        range: impl IntoRange,
        rows: impl IntoIterator<Item = R>,
    ) -> Result<()> {
        let range = range.into_range()?;
        let rows: Vec<Vec<CellInput>> = rows.into_iter().map(IntoRow::into_cells).collect();
        if !range.is_cell() {
            let width = range.width() as usize;
            if rows.len() != range.height() as usize || rows.iter().any(|row| row.len() != width) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "The values do not fit {range}: it needs {} rows of {width}",
                        range.height()
                    ),
                ));
            }
        }
        let (top, left) = (range.start().row(), range.start().column());
        for (r, row) in rows.into_iter().enumerate() {
            for (c, input) in row.into_iter().enumerate() {
                let at = CellRef::new(top + r as u32, left + c as u32)?;
                self.set_value(at, input)?;
            }
        }
        Ok(())
    }
    /// Append a row after the last row that has cells. Returns its
    /// zero-based row index.
    pub fn append_row(&mut self, row: impl IntoRow) -> Result<u32> {
        // The engine gives each date or time the default format of its kind.
        let values = row
            .into_cells()
            .into_iter()
            .map(to_core)
            .collect::<Result<Vec<_>>>()?;
        let id = self.id;
        let index = self
            .book
            .engine
            .append(id, values)
            .map_err(|error| self.context(error))?;
        Ok(index.get())
    }
    /// Append rows after the last row that has cells.
    pub fn append_rows<R: IntoRow>(&mut self, rows: impl IntoIterator<Item = R>) -> Result<()> {
        for row in rows {
            self.append_row(row)?;
        }
        Ok(())
    }
    /// Clear the values and formulas of a range. Styles stay.
    pub fn clear(&mut self, range: impl IntoRange) -> Result<()> {
        let range = range.into_range()?;
        let present: Vec<CellAddress> = self
            .model()?
            .cells()
            .filter(|cell| range.0.contains(cell.address))
            .map(|cell| cell.address)
            .collect();
        for address in present {
            self.set_value(CellRef(address), Value::Blank)?;
        }
        Ok(())
    }

    // ----- Styles -----

    /// Give every cell of a range the same style.
    pub fn set_style(&mut self, range: impl IntoRange, style: &Style) -> Result<()> {
        let range = range.into_range()?;
        let id = self.book.style_id(style)?;
        for at in range.cells() {
            self.book.engine.set_style(self.id, at.0, id)?;
        }
        Ok(())
    }
    /// Change the style of each cell of a range: `change` receives the
    /// current style and returns the new one.
    pub fn map_style(
        &mut self,
        range: impl IntoRange,
        mut change: impl FnMut(Style) -> Style,
    ) -> Result<()> {
        let range = range.into_range()?;
        for at in range.cells() {
            let style = change(self.style(at)?);
            self.set_style(at, &style)?;
        }
        Ok(())
    }

    // ----- Layout -----

    /// Set the width of columns, in characters of the default font.
    pub fn set_column_width(&mut self, columns: impl IntoSpan, width: f64) -> Result<()> {
        if !(0.0..=255.0).contains(&width) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A column width is between 0 and 255",
            ));
        }
        let span = columns.into_column_span()?;
        self.edit_columns(span, |column| {
            column.width = Some(width);
            column.custom_width = Some(true);
        })
    }
    /// Hide or show columns.
    pub fn set_columns_hidden(&mut self, columns: impl IntoSpan, hidden: bool) -> Result<()> {
        let span = columns.into_column_span()?;
        self.edit_columns(span, |column| column.hidden = Some(hidden))
    }
    fn edit_columns(&mut self, span: Span, change: impl Fn(&mut ColumnDimension)) -> Result<()> {
        let existing = self.model()?.dimensions().columns().to_vec();
        for index in span.start..span.start + span.count {
            let mut column = existing
                .iter()
                .find(|column| (column.start.get()..=column.end.get()).contains(&index))
                .cloned()
                .unwrap_or(ColumnDimension::new(
                    ColumnIndex::new(index)?,
                    ColumnIndex::new(index)?,
                )?);
            column.start = ColumnIndex::new(index)?;
            column.end = ColumnIndex::new(index)?;
            change(&mut column);
            self.book.engine.set_column_dimension(self.id, column)?;
        }
        Ok(())
    }
    /// The width of a column, if it is set.
    pub fn column_width(&mut self, column: &str) -> Result<Option<f64>> {
        let span = Span::columns(column)?;
        Ok(self
            .model()?
            .dimensions()
            .column(ColumnIndex::new(span.start)?)
            .and_then(|column| column.width))
    }
    /// Set the height of rows, in points.
    pub fn set_row_height(&mut self, rows: impl IntoSpan, height: f64) -> Result<()> {
        if !(0.0..=409.0).contains(&height) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A row height is between 0 and 409 points",
            ));
        }
        let span = rows.into_row_span()?;
        self.edit_rows(span, |row| {
            row.height = Some(height);
            row.custom_height = Some(true);
        })
    }
    /// Hide or show rows.
    pub fn set_rows_hidden(&mut self, rows: impl IntoSpan, hidden: bool) -> Result<()> {
        let span = rows.into_row_span()?;
        self.edit_rows(span, |row| row.hidden = Some(hidden))
    }
    fn edit_rows(&mut self, span: Span, change: impl Fn(&mut RowDimension)) -> Result<()> {
        for index in span.start..span.start + span.count {
            let index = RowIndex::new(index)?;
            let mut row = self
                .model()?
                .dimensions()
                .row(index)
                .cloned()
                .unwrap_or(RowDimension::new(index));
            change(&mut row);
            self.book.engine.set_row_dimension(self.id, row)?;
        }
        Ok(())
    }
    /// The height of a row in points, if it is set.
    pub fn row_height(&mut self, row: &str) -> Result<Option<f64>> {
        let span = Span::rows(row)?;
        Ok(self
            .model()?
            .dimensions()
            .row(RowIndex::new(span.start)?)
            .and_then(|row| row.height))
    }
    /// Freeze the rows above and the columns left of a cell. `"A2"` freezes
    /// the first row; `"B1"` the first column.
    pub fn freeze_panes(&mut self, at: impl IntoCellRef) -> Result<()> {
        let at = at.into_cell_ref()?;
        let mut views = self.book.engine.sheet_views(self.id)?;
        if views.views.is_empty() {
            views.views.push(crabxl_core::SheetView::default());
        }
        let view = &mut views.views[0];
        view.pane = if at.row() == 0 && at.column() == 0 {
            None
        } else {
            let position = match (at.row() > 0, at.column() > 0) {
                (true, true) => crabxl_core::PanePosition::BottomRight,
                (true, false) => crabxl_core::PanePosition::BottomLeft,
                _ => crabxl_core::PanePosition::TopRight,
            };
            Some(Box::new(crabxl_core::Pane {
                x_split: (at.column() > 0).then(|| f64::from(at.column())),
                y_split: (at.row() > 0).then(|| f64::from(at.row())),
                top_left_cell: Some(at.to_string().into()),
                active_pane: position,
                state: crabxl_core::PaneState::Frozen,
            }))
        };
        view.selections.clear();
        self.book.engine.set_sheet_views(self.id, views)
    }
    /// The cell below and right of the frozen panes, if panes are frozen.
    pub fn frozen_panes(&mut self) -> Result<Option<CellRef>> {
        let views = self.book.engine.sheet_views(self.id)?;
        let Some(pane) = views.views.first().and_then(|view| view.pane.as_deref()) else {
            return Ok(None);
        };
        if pane.state != crabxl_core::PaneState::Frozen {
            return Ok(None);
        }
        let row = pane.y_split.unwrap_or(0.0) as u32;
        let column = pane.x_split.unwrap_or(0.0) as u32;
        CellRef::new(row, column).map(Some)
    }
    /// Remove frozen panes.
    pub fn unfreeze_panes(&mut self) -> Result<()> {
        self.freeze_panes("A1")
    }

    // ----- Structure -----

    /// Insert rows before the first row of `rows`, such as `"5:7"` for three
    /// rows before row 5. Formulas, names, ranges and objects that refer to
    /// moved cells follow them. Content that cannot follow, such as a pivot
    /// table on this sheet, fails with `ErrorKind::Unsupported` before any
    /// change.
    pub fn insert_rows(&mut self, rows: impl IntoSpan) -> Result<()> {
        let span = rows.into_row_span()?;
        let id = self.id;
        self.book
            .engine
            .insert_rows(id, RowIndex::new(span.start)?, span.count)
            .map_err(|error| self.context(error))
    }
    /// Delete rows, such as `"5:7"`. References into deleted cells become
    /// `#REF!`. See [`Self::insert_rows`].
    pub fn delete_rows(&mut self, rows: impl IntoSpan) -> Result<()> {
        let span = rows.into_row_span()?;
        let id = self.id;
        self.book
            .engine
            .delete_rows(id, RowIndex::new(span.start)?, span.count)
            .map_err(|error| self.context(error))
    }
    /// Insert columns before the first column of `columns`, such as `"B:C"`.
    /// See [`Self::insert_rows`].
    pub fn insert_columns(&mut self, columns: impl IntoSpan) -> Result<()> {
        let span = columns.into_column_span()?;
        let id = self.id;
        self.book
            .engine
            .insert_columns(id, ColumnIndex::new(span.start)?, span.count)
            .map_err(|error| self.context(error))
    }
    /// Delete columns, such as `"B:C"`. See [`Self::delete_rows`].
    pub fn delete_columns(&mut self, columns: impl IntoSpan) -> Result<()> {
        let span = columns.into_column_span()?;
        let id = self.id;
        self.book
            .engine
            .delete_columns(id, ColumnIndex::new(span.start)?, span.count)
            .map_err(|error| self.context(error))
    }

    // ----- Merges -----

    /// Merge a range. Only the top-left cell keeps its value.
    pub fn merge(&mut self, range: impl IntoRange) -> Result<()> {
        let range = range.into_range()?;
        if range.is_cell() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Merge at least two cells",
            ));
        }
        self.book.engine.merge_cells(self.id, range.0)
    }
    /// Split a merged range.
    pub fn unmerge(&mut self, range: impl IntoRange) -> Result<()> {
        let range = range.into_range()?;
        if !self.merged_ranges()?.contains(&range) {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("{range} is not merged"),
            ));
        }
        self.book.engine.unmerge_cells(self.id, range.0)
    }
    /// The merged ranges.
    pub fn merged_ranges(&mut self) -> Result<Vec<RangeRef>> {
        Ok(self
            .model()?
            .merged_ranges()
            .ranges()
            .iter()
            .map(|merge| RangeRef(merge.range()))
            .collect())
    }

    // ----- Objects -----

    fn features(&mut self) -> Result<crabxl_core::SheetFeatures> {
        Ok(self.book.engine.sheet_features(self.id)?.clone())
    }
    /// The tables.
    pub fn tables(&mut self) -> Result<Vec<Table>> {
        self.book
            .engine
            .sheet_features(self.id)?
            .tables
            .iter()
            .map(Table::from_core)
            .collect()
    }
    /// Add a table. The header cells must hold distinct, non-empty text;
    /// they name the columns.
    pub fn add_table(&mut self, table: Table) -> Result<()> {
        let (taken, id) = self.book.table_names()?;
        if taken.contains(&table.name.to_lowercase()) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("A table named '{}' already exists", table.name),
            ));
        }
        let header = RangeRef::new(
            table.range.start(),
            CellRef::new(table.range.start().row(), table.range.end().column())?,
        )?;
        let mut headers = Vec::new();
        for value in self.values(header)?.rows().flatten() {
            let text = match value {
                Value::Text(text) => text.as_str().to_owned(),
                Value::Number(number) => number.to_exact_string(),
                _ => String::new(),
            };
            if text.is_empty()
                || headers
                    .iter()
                    .any(|other: &String| other.eq_ignore_ascii_case(&text))
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "The header cells of table '{}' need distinct, non-empty text",
                        table.name
                    ),
                ));
            }
            headers.push(text);
        }
        if table.totals_row && table.range.height() < 3 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A table with a totals row needs a header row, a data row and the totals row",
            ));
        }
        let mut core_table = table.to_core(id, headers)?;
        if table.totals_row {
            // Excel rejects a totals row whose cells the columns do not declare.
            let row = table.range.end().row();
            for (offset, column) in (0u32..).zip(core_table.table_columns.iter_mut()) {
                let at = CellRef::new(row, table.range.start().column() + offset)?;
                if let Some(formula) = self.formula(at)? {
                    column.totals_row_function =
                        Some(crabxl_core::features::TotalsRowFunction::Custom);
                    column.totals_row_formula = Some(crabxl_core::features::TableFormula {
                        array: None,
                        text: formula.text().into(),
                    });
                    continue;
                }
                match self.value(at)? {
                    Value::Blank => {}
                    Value::Text(text) => column.totals_row_label = Some(text.as_str().into()),
                    _ => {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Totals row cell {at} needs text, a formula or nothing"),
                        ));
                    }
                }
            }
        }
        let mut features = self.features()?;
        if features.tables.iter().any(|other| {
            other
                .ref_range
                .parse::<crabxl_core::CellRange>()
                .is_ok_and(|range| range.intersects(table.range.0))
        }) {
            return Err(Error::new(ErrorKind::InvalidData, "Tables cannot overlap"));
        }
        if features
            .auto_filter
            .as_ref()
            .and_then(|filter| filter.ref_range.as_deref())
            .and_then(|text| text.parse::<crabxl_core::CellRange>().ok())
            .is_some_and(|filter| filter.intersects(table.range.0))
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A table cannot overlap the worksheet filter",
            ));
        }
        features.tables.push(core_table);
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// Remove a table by name. Its cells keep their values.
    pub fn remove_table(&mut self, name: &str) -> Result<()> {
        let mut features = self.features()?;
        let before = features.tables.len();
        features
            .tables
            .retain(|table| !table.display_name.eq_ignore_ascii_case(name));
        if features.tables.len() == before {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("No table named '{name}'"),
            ));
        }
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// The data validation rules.
    pub fn data_validations(&mut self) -> Result<Vec<DataValidation>> {
        Ok(self
            .book
            .engine
            .sheet_features(self.id)?
            .data_validations
            .validations
            .iter()
            .map(|inner| DataValidation {
                inner: inner.clone(),
            })
            .collect())
    }
    /// Add a data validation rule. It needs at least one range.
    pub fn add_data_validation(&mut self, validation: DataValidation) -> Result<()> {
        if validation.inner.ranges.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Give the validation a range with with_range",
            ));
        }
        let mut features = self.features()?;
        features.data_validations.validations.push(validation.inner);
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// Remove every data validation rule that applies to a cell of `range`.
    pub fn remove_data_validations(&mut self, range: impl IntoRange) -> Result<usize> {
        let range = range.into_range()?;
        let mut features = self.features()?;
        let before = features.data_validations.validations.len();
        features
            .data_validations
            .validations
            .retain(|rule| !rule.ranges.iter().any(|other| other.intersects(range.0)));
        let removed = before - features.data_validations.validations.len();
        if removed > 0 {
            self.book.engine.set_sheet_features(self.id, features)?;
        }
        Ok(removed)
    }
    /// The conditional formats, in priority order.
    pub fn conditional_formats(&mut self) -> Result<Vec<ConditionalFormat>> {
        let styles = self.book.engine.differential_styles().to_vec();
        let mut output = Vec::new();
        for block in &self
            .book
            .engine
            .sheet_features(self.id)?
            .conditional_formats
        {
            for rule in &block.rules {
                output.push(ConditionalFormat {
                    ranges: block.ranges.iter().copied().map(RangeRef).collect(),
                    rule: rule.clone(),
                    style: rule
                        .dxf_id
                        .and_then(|id| styles.get(id as usize))
                        .map(|inner| crate::style::PartialStyle {
                            inner: inner.clone(),
                        }),
                });
            }
        }
        output.sort_by_key(|format| format.rule.priority.unwrap_or(u32::MAX));
        Ok(output)
    }
    /// Add a conditional format with the lowest priority.
    pub fn add_conditional_format(&mut self, format: ConditionalFormat) -> Result<()> {
        let mut rule = format.rule;
        if let Some(style) = format.style {
            rule.dxf_id = Some(self.book.engine.register_differential_style(style.inner)?);
        }
        let mut features = self.features()?;
        let priority = features
            .conditional_formats
            .iter()
            .flat_map(|block| &block.rules)
            .filter_map(|rule| rule.priority)
            .max()
            .unwrap_or(0)
            + 1;
        rule.priority = Some(priority);
        features
            .conditional_formats
            .push(crabxl_core::features::ConditionalFormatting {
                ranges: format.ranges.iter().map(|range| range.0).collect(),
                pivot: None,
                rules: vec![rule],
            });
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// The note of a cell.
    pub fn note(&mut self, at: impl IntoCellRef) -> Result<Option<Note>> {
        let at = at.into_cell_ref()?;
        Ok(self
            .book
            .engine
            .sheet_features(self.id)?
            .comments
            .iter()
            .find(|comment| comment.address == at.0)
            .map(|comment| Note::new(&comment.text, &comment.author)))
    }
    /// Set or replace the note of a cell.
    pub fn set_note(&mut self, at: impl IntoCellRef, note: Note) -> Result<()> {
        let at = at.into_cell_ref()?;
        let mut features = self.features()?;
        features.comments.retain(|comment| comment.address != at.0);
        features.comments.push(crabxl_core::features::Comment::new(
            at.0,
            note.text,
            note.author,
        ));
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// Remove the note of a cell. Returns whether it had one.
    pub fn remove_note(&mut self, at: impl IntoCellRef) -> Result<bool> {
        let at = at.into_cell_ref()?;
        let mut features = self.features()?;
        let before = features.comments.len();
        features.comments.retain(|comment| comment.address != at.0);
        let removed = features.comments.len() != before;
        if removed {
            self.book.engine.set_sheet_features(self.id, features)?;
        }
        Ok(removed)
    }
    /// The hyperlink of a cell.
    pub fn hyperlink(&mut self, at: impl IntoCellRef) -> Result<Option<Hyperlink>> {
        let at = at.into_cell_ref()?;
        Ok(self
            .book
            .engine
            .hyperlinks(self.id)?
            .get(at.0)
            .and_then(Hyperlink::from_core))
    }
    /// Set or replace the hyperlink of a cell.
    pub fn set_hyperlink(&mut self, at: impl IntoCellRef, link: Hyperlink) -> Result<()> {
        let at = at.into_cell_ref()?;
        self.book
            .engine
            .set_hyperlink(self.id, at.0, Some(link.to_core()))
    }
    /// Remove the hyperlink of a cell. The cell keeps its value.
    pub fn remove_hyperlink(&mut self, at: impl IntoCellRef) -> Result<()> {
        let at = at.into_cell_ref()?;
        self.book.engine.update_hyperlink(self.id, at.0, None)
    }
    /// Place a picture with its top-left corner on a cell, at its natural size.
    pub fn add_image(&mut self, image: &Image, at: impl IntoCellRef) -> Result<()> {
        let mut images = self.book.engine.sheet_images(self.id)?.to_vec();
        let mut picture = image.to_core(at)?;
        picture.name = format!("Picture {}", images.len() + 1).into();
        images.push(picture);
        self.book.engine.set_sheet_images(self.id, images)
    }
    /// The number of pictures on the sheet.
    pub fn image_count(&mut self) -> Result<usize> {
        Ok(self.book.engine.sheet_images(self.id)?.len())
    }
    /// Protect the sheet against edits, optionally with a password. Cells
    /// stay editable when their style has `locked` set to false.
    pub fn protect(&mut self, password: Option<&str>) -> Result<()> {
        let mut features = self.features()?;
        let mut protection = crabxl_core::features::SheetProtection {
            sheet: true,
            ..Default::default()
        };
        if let Some(password) = password {
            protection.set_password(password);
        }
        features.protection = Some(protection);
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// Remove sheet protection.
    pub fn unprotect(&mut self) -> Result<()> {
        let mut features = self.features()?;
        features.protection = None;
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// Whether the sheet is protected.
    pub fn is_protected(&mut self) -> Result<bool> {
        Ok(self
            .book
            .engine
            .sheet_features(self.id)?
            .protection
            .as_ref()
            .is_some_and(|protection| protection.sheet))
    }

    /// The pictures, with the cell of each top-left corner.
    pub fn images(&mut self) -> Result<Vec<PlacedImage>> {
        self.book
            .engine
            .sheet_images(self.id)?
            .iter()
            .map(PlacedImage::from_core)
            .collect()
    }
    /// Remove every picture whose top-left corner is in `range`. Returns the
    /// count.
    pub fn remove_images(&mut self, range: impl IntoRange) -> Result<usize> {
        let range = range.into_range()?;
        let mut images = self.book.engine.sheet_images(self.id)?.to_vec();
        let before = images.len();
        images.retain(|image| !anchored_in(&image.anchor, range));
        let removed = before - images.len();
        if removed > 0 {
            self.book.engine.set_sheet_images(self.id, images)?;
        }
        Ok(removed)
    }
    /// The charts.
    pub fn charts(&mut self) -> Result<Vec<Chart>> {
        Ok(self
            .book
            .engine
            .sheet_charts(self.id)?
            .iter()
            .map(Chart::from_core)
            .collect())
    }
    /// Add a chart. Its series refer to cells; Excel draws them when it
    /// opens the file.
    pub fn add_chart(&mut self, chart: &Chart) -> Result<()> {
        let mut charts = self.book.engine.sheet_charts(self.id)?.to_vec();
        let mut core = chart.to_core()?;
        core.name = format!("Chart {}", charts.len() + 1).into();
        charts.push(core);
        self.book.engine.set_sheet_charts(self.id, charts)
    }
    /// Remove every chart whose top-left corner is in `range`. Returns the
    /// count.
    pub fn remove_charts(&mut self, range: impl IntoRange) -> Result<usize> {
        let range = range.into_range()?;
        let mut charts = self.book.engine.sheet_charts(self.id)?.to_vec();
        let before = charts.len();
        charts.retain(|chart| !anchored_in(&chart.anchor, range));
        let removed = before - charts.len();
        if removed > 0 {
            self.book.engine.set_sheet_charts(self.id, charts)?;
        }
        Ok(removed)
    }
    /// Remove every conditional format that applies to a cell of `range`.
    /// Returns the count of rules removed.
    pub fn remove_conditional_formats(&mut self, range: impl IntoRange) -> Result<usize> {
        let range = range.into_range()?;
        let mut features = self.features()?;
        let mut removed = 0;
        features.conditional_formats.retain(|block| {
            let hit = block.ranges.iter().any(|other| other.intersects(range.0));
            if hit {
                removed += block.rules.len();
            }
            !hit
        });
        if removed > 0 {
            self.book.engine.set_sheet_features(self.id, features)?;
        }
        Ok(removed)
    }

    // ----- Filter, outline and printing -----

    /// The range with filter buttons, if any.
    pub fn auto_filter(&mut self) -> Result<Option<RangeRef>> {
        Ok(self
            .book
            .engine
            .sheet_features(self.id)?
            .auto_filter
            .as_ref()
            .and_then(|filter| filter.ref_range.as_deref())
            .and_then(|text| text.into_range().ok()))
    }
    /// Put filter buttons on the header row of a range. Replaces any
    /// worksheet filter and its criteria. Tables have their own filters, so
    /// the range cannot overlap a table.
    pub fn set_auto_filter(&mut self, range: impl IntoRange) -> Result<()> {
        let range = range.into_range()?;
        let mut features = self.features()?;
        if features.tables.iter().any(|table| {
            table
                .ref_range
                .parse::<crabxl_core::CellRange>()
                .is_ok_and(|other| other.intersects(range.0))
        }) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A worksheet filter cannot overlap a table",
            ));
        }
        features.auto_filter = Some(crabxl_core::features::AutoFilter {
            ref_range: Some(range.to_string().into()),
            ..Default::default()
        });
        self.book.engine.set_sheet_features(self.id, features)
    }
    /// Remove the worksheet filter.
    pub fn remove_auto_filter(&mut self) -> Result<()> {
        let mut features = self.features()?;
        if features.auto_filter.take().is_some() {
            self.book.engine.set_sheet_features(self.id, features)?;
        }
        Ok(())
    }
    /// Set the outline level of rows, from 0 (not grouped) to 7.
    pub fn set_rows_outline_level(&mut self, rows: impl IntoSpan, level: u8) -> Result<()> {
        let level = outline_level(level)?;
        let span = rows.into_row_span()?;
        self.edit_rows(span, |row| row.outline_level = level)
    }
    /// The outline level of a row.
    pub fn row_outline_level(&mut self, row: &str) -> Result<u8> {
        let span = Span::rows(row)?;
        let level = self
            .model()?
            .dimensions()
            .row(RowIndex::new(span.start)?)
            .and_then(|row| row.outline_level);
        Ok(level.map_or(0, |level| level.min(7) as u8))
    }
    /// Set the outline level of columns, from 0 (not grouped) to 7.
    pub fn set_columns_outline_level(&mut self, columns: impl IntoSpan, level: u8) -> Result<()> {
        let level = outline_level(level)?;
        let span = columns.into_column_span()?;
        self.edit_columns(span, |column| column.outline_level = level)
    }
    /// The outline level of a column.
    pub fn column_outline_level(&mut self, column: &str) -> Result<u8> {
        let span = Span::columns(column)?;
        let level = self
            .model()?
            .dimensions()
            .column(ColumnIndex::new(span.start)?)
            .and_then(|column| column.outline_level);
        Ok(level.map_or(0, |level| level.min(7) as u8))
    }
    /// The color of the sheet tab, if set.
    pub fn tab_color(&mut self) -> Result<Option<Color>> {
        let settings = self.book.engine.print_settings(self.id)?;
        settings
            .properties
            .as_ref()
            .and_then(|properties| properties.tab_color.as_ref())
            .map(color_from_attributes)
            .transpose()
    }
    /// Set or clear the color of the sheet tab.
    pub fn set_tab_color(&mut self, color: Option<&Color>) -> Result<()> {
        let tab = color.map(color_attributes).transpose()?;
        let mut settings = self.book.engine.print_settings(self.id)?;
        let mut properties = settings.properties.take().unwrap_or_default();
        properties.tab_color = tab;
        settings.properties = Some(properties);
        self.book.engine.set_print_settings(self.id, settings)
    }
    /// How the sheet prints.
    pub fn page_setup(&mut self) -> Result<PageSetup> {
        use crabxl_core::PageOrientation as O;
        let settings = self.book.engine.print_settings(self.id)?;
        let setup = &settings.setup;
        let count = |value: Option<i64>| value.and_then(|v| u32::try_from(v).ok());
        let (print_area, titles) = self.print_names()?;
        let (title_rows, title_columns) = titles;
        Ok(PageSetup {
            orientation: match setup.orientation {
                Some(O::Portrait) => Some(Orientation::Portrait),
                Some(O::Landscape) => Some(Orientation::Landscape),
                _ => None,
            },
            paper_size: count(setup.paper_size),
            scale: count(setup.scale),
            fit_to: (settings.fit_to_page == Some(true)).then(|| {
                (
                    count(setup.fit_to_width).unwrap_or(1),
                    count(setup.fit_to_height).unwrap_or(1),
                )
            }),
            margins: settings.margins.map_or_else(Margins::default, |m| Margins {
                left: m.left,
                right: m.right,
                top: m.top,
                bottom: m.bottom,
                header: m.header,
                footer: m.footer,
            }),
            center_horizontally: settings.options.horizontal_centered == Some(true),
            center_vertically: settings.options.vertical_centered == Some(true),
            grid_lines: settings.options.grid_lines == Some(true),
            print_area,
            title_rows,
            title_columns,
        })
    }
    /// Set how the sheet prints. Settings that `PageSetup` does not cover,
    /// such as page breaks and headers, are kept.
    pub fn set_page_setup(&mut self, page: &PageSetup) -> Result<()> {
        use crabxl_core::PageOrientation as O;
        page.validate()?;
        let mut settings = self.book.engine.print_settings(self.id)?;
        let setup = &mut settings.setup;
        setup.orientation = page.orientation.map(|orientation| match orientation {
            Orientation::Portrait => O::Portrait,
            Orientation::Landscape => O::Landscape,
        });
        setup.paper_size = page.paper_size.map(i64::from);
        setup.scale = page.scale.map(i64::from);
        setup.fit_to_width = page.fit_to.map(|(wide, _)| i64::from(wide));
        setup.fit_to_height = page.fit_to.map(|(_, tall)| i64::from(tall));
        settings.fit_to_page = page.fit_to.map(|_| true);
        let m = page.margins;
        settings.margins = Some(crabxl_core::PageMargins {
            left: m.left,
            right: m.right,
            top: m.top,
            bottom: m.bottom,
            header: m.header,
            footer: m.footer,
        });
        settings.options.horizontal_centered = page.center_horizontally.then_some(true);
        settings.options.vertical_centered = page.center_vertically.then_some(true);
        settings.options.grid_lines = page.grid_lines.then_some(true);
        self.book.engine.set_print_settings(self.id, settings)?;
        self.set_print_names(page)
    }
    fn sheet_scope(&mut self) -> Result<u32> {
        let id = self.id;
        self.book
            .ids()
            .iter()
            .position(|(other, _)| *other == id)
            .map(|index| index as u32)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "The sheet was removed"))
    }
    #[allow(clippy::type_complexity)]
    fn print_names(&mut self) -> Result<(Option<RangeRef>, (Option<Span>, Option<Span>))> {
        let scope = self.sheet_scope()?;
        let features = self.book.engine.workbook_features()?;
        let find = |name: &str| {
            features
                .defined_names
                .iter()
                .find(|d| d.local_sheet_id == Some(scope) && d.name.eq_ignore_ascii_case(name))
                .map(|d| d.value.to_string())
        };
        let local = |part: &str| part.rsplit('!').next().unwrap_or(part).replace('$', "");
        // shortcut: an area of several ranges reads as None, upgrade if needed
        let area = find(PRINT_AREA).and_then(|value| local(&value).into_range().ok());
        let mut titles = (None, None);
        for part in find(PRINT_TITLES).iter().flat_map(|value| value.split(',')) {
            let part = local(part);
            if part.starts_with(|c: char| c.is_ascii_digit()) {
                titles.0 = Span::rows(&part).ok();
            } else {
                titles.1 = Span::columns(&part).ok();
            }
        }
        Ok((area, titles))
    }
    fn set_print_names(&mut self, page: &PageSetup) -> Result<()> {
        let scope = self.sheet_scope()?;
        let sheet = format!("'{}'", self.name().replace('\'', "''"));
        let column = |index: u32| -> Result<String> { Ok(CellRef::new(0, index)?.column_name()) };
        let area = page
            .print_area
            .map(|range| -> Result<String> {
                let (start, end) = (range.start(), range.end());
                Ok(format!(
                    "{sheet}!${}${}:${}${}",
                    column(start.column())?,
                    start.row() + 1,
                    column(end.column())?,
                    end.row() + 1
                ))
            })
            .transpose()?;
        let mut titles = Vec::new();
        if let Some(rows) = page.title_rows {
            titles.push(format!(
                "{sheet}!${}:${}",
                rows.start + 1,
                rows.start + rows.count
            ));
        }
        if let Some(columns) = page.title_columns {
            titles.push(format!(
                "{sheet}!${}:${}",
                column(columns.start)?,
                column(columns.start + columns.count - 1)?
            ));
        }
        let titles = (!titles.is_empty()).then(|| titles.join(","));
        let mut features = self.book.engine.workbook_features()?.clone();
        for (name, value) in [(PRINT_AREA, area), (PRINT_TITLES, titles)] {
            features.defined_names.retain(|d| {
                !(d.local_sheet_id == Some(scope) && d.name.eq_ignore_ascii_case(name))
            });
            if let Some(value) = value {
                features
                    .defined_names
                    .push(crabxl_core::features::DefinedName {
                        name: name.into(),
                        value: value.into(),
                        local_sheet_id: Some(scope),
                        ..Default::default()
                    });
            }
        }
        self.book.engine.set_workbook_features(features)
    }
}

const PRINT_AREA: &str = "_xlnm.Print_Area";
const PRINT_TITLES: &str = "_xlnm.Print_Titles";

fn anchored_in(anchor: &crabxl_core::DrawingAnchor, range: RangeRef) -> bool {
    match anchor {
        crabxl_core::DrawingAnchor::OneCell { from, .. }
        | crabxl_core::DrawingAnchor::TwoCell { from, .. } => {
            let (start, end) = (range.start(), range.end());
            (start.row()..=end.row()).contains(&from.row)
                && (start.column()..=end.column()).contains(&from.col)
        }
        crabxl_core::DrawingAnchor::Absolute { .. } => false,
    }
}

fn outline_level(level: u8) -> Result<Option<u32>> {
    if level > 7 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "An outline level is from 0 to 7",
        ));
    }
    Ok((level > 0).then_some(u32::from(level)))
}

fn color_attributes(color: &Color) -> Result<crabxl_core::ElementAttributes> {
    use crabxl_core::ColorKind;
    let mut attributes: crabxl_core::ElementAttributes = Vec::new();
    let mut push = |name: &str, value: String| attributes.push((name.into(), value.into()));
    match &color.kind {
        ColorKind::Theme(index) => push("theme", index.as_i64().unwrap_or(0).to_string()),
        ColorKind::Indexed(index) => push("indexed", index.as_i64().unwrap_or(0).to_string()),
        _ => match color.to_rgb() {
            Some((r, g, b)) => push("rgb", format!("FF{r:02X}{g:02X}{b:02X}")),
            None => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "A tab color is an RGB, theme or indexed color",
                ));
            }
        },
    }
    if let Some(tint) = color.tint {
        push("tint", tint.to_string());
    }
    Ok(attributes)
}

fn color_from_attributes(attributes: &crabxl_core::ElementAttributes) -> Result<Color> {
    use crabxl_core::ColorKind;
    let get = |name: &str| {
        attributes
            .iter()
            .find(|(key, _)| key.as_ref() == name)
            .map(|(_, value)| value.as_ref())
    };
    let number = |text: &str| {
        text.parse::<i64>()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Invalid tab color index"))
    };
    let mut color = if let Some(rgb) = get("rgb") {
        Color::hex(rgb)?
    } else if let Some(theme) = get("theme") {
        Color {
            kind: ColorKind::Theme(number(theme)?.into()),
            tint: None,
        }
    } else if let Some(indexed) = get("indexed") {
        Color {
            kind: ColorKind::Indexed(number(indexed)?.into()),
            tint: None,
        }
    } else {
        Color {
            kind: ColorKind::Auto(true),
            tint: None,
        }
    };
    color.tint = get("tint").and_then(|tint| tint.parse().ok());
    Ok(color)
}

impl std::fmt::Debug for Worksheet<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worksheet")
            .field("name", &self.name())
            .finish()
    }
}
