//! Streaming writes with bounded memory.
//!
//! A [`Writer`] spools each row to a temporary file, so it writes sheets of
//! any size in a small amount of memory. Rows go in order; a written row
//! cannot change.
//!
//! ```no_run
//! use crabxl::style::{Font, Style};
//! use crabxl::write::{WriteOptions, Writer};
//! # fn main() -> crabxl::Result<()> {
//! let header = Style::new().with_font(Font::new().with_bold(true));
//! let mut writer = Writer::create("export.xlsx", &WriteOptions::new())?;
//! let mut sheet = writer.add_worksheet("Events")?;
//! sheet.set_column_width("A:C", 16.0)?;
//! sheet.freeze_panes("A2")?;
//! sheet.append_row_styled(("id", "kind", "amount"), &header)?;
//! for id in 0..1_000_000_i64 {
//!     sheet.append_row((id, "sale", 9.5))?;
//! }
//! writer.finish()?;
//! # Ok(())
//! # }
//! ```

use crate::features::{ConditionalFormat, DataValidation};
use crate::reference::{CellRef, IntoCellRef, IntoRange, IntoSpan};
use crate::row::IntoRow;
use crate::style::Style;
use crate::value::{CellInput, Value, to_core};
use crate::{Error, ErrorKind, Result};
use crabxl_core::{Cell, ColumnDimension, ColumnIndex, RowIndex, StyleId};
use crabxl_xlsx::WorkbookWriter;
use std::collections::HashMap;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

/// Options for a [`Writer`].
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct WriteOptions {
    /// ZIP compression level: 0 stores, 1 to 9 deflate. `None` uses 6.
    pub compression: Option<u8>,
    /// Directory for temporary files. `None` uses the system directory.
    pub temp_directory: Option<PathBuf>,
}

impl WriteOptions {
    /// The default options.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the compression level, 0 to 9.
    pub fn with_compression(mut self, level: u8) -> Self {
        self.compression = Some(level);
        self
    }
    /// Put temporary files in a directory.
    pub fn with_temp_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.temp_directory = Some(directory.into());
        self
    }
}

#[derive(Default)]
struct SheetState {
    name: String,
    next_row: u32,
    rows_written: bool,
    features: crabxl_core::SheetFeatures,
    merges: Vec<crabxl_core::CellRange>,
    closed: bool,
}

/// Writes a new workbook sheet by sheet and row by row.
pub struct Writer {
    inner: WorkbookWriter,
    target: Option<PathBuf>,
    sheets: Vec<(usize, SheetState)>,
    styles: HashMap<usize, (Style, StyleId)>,
    temporal: HashMap<(usize, &'static str), Style>,
}

impl Writer {
    /// Write to a path. [`Writer::finish`] replaces the file atomically.
    pub fn create(path: impl AsRef<Path>, options: &WriteOptions) -> Result<Self> {
        let mut writer = Self::new(options)?;
        writer.target = Some(path.as_ref().to_owned());
        Ok(writer)
    }
    /// Write to a target you pass to [`Writer::finish_into`].
    pub fn new(options: &WriteOptions) -> Result<Self> {
        let inner = WorkbookWriter::new(crabxl_xlsx::WriteOptions {
            compression_level: options.compression,
            temp_directory: options.temp_directory.clone(),
            ..Default::default()
        })?;
        Ok(Self {
            inner,
            target: None,
            sheets: Vec::new(),
            styles: HashMap::new(),
            temporal: HashMap::new(),
        })
    }
    /// Start a new worksheet after the last one. Earlier worksheets stay
    /// open: you can return to them with [`Writer::worksheet`].
    pub fn add_worksheet(&mut self, name: &str) -> Result<WorksheetWriter<'_>> {
        if self
            .sheets
            .iter()
            .any(|(_, sheet)| sheet.name.eq_ignore_ascii_case(name))
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("A sheet named '{name}' already exists"),
            ));
        }
        let id = self.inner.start_interleaved_sheet(name)?;
        self.sheets.push((
            id,
            SheetState {
                name: name.to_owned(),
                ..Default::default()
            },
        ));
        Ok(WorksheetWriter { writer: self, id })
    }
    /// An open worksheet, to write more rows to it.
    pub fn worksheet(&mut self, name: &str) -> Result<WorksheetWriter<'_>> {
        let id = self
            .sheets
            .iter()
            .find(|(_, sheet)| sheet.name.eq_ignore_ascii_case(name))
            .map(|(id, sheet)| (*id, sheet.closed))
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, format!("No sheet named '{name}'"))
            })?;
        if id.1 {
            return Err(Error::new(
                ErrorKind::InvalidState,
                format!("Sheet '{name}' is finished"),
            ));
        }
        Ok(WorksheetWriter {
            writer: self,
            id: id.0,
        })
    }
    fn style_id(&mut self, style: &Style) -> Result<StyleId> {
        let key = style.identity();
        if let Some((_, id)) = self.styles.get(&key) {
            return Ok(*id);
        }
        let id = self.inner.register_style(style.to_core())?;
        self.styles.insert(key, (style.clone(), id));
        Ok(id)
    }
    /// The style for a date or time cell: `base` with the default format of
    /// its kind. Each combination is created once.
    fn temporal_style(&mut self, base: &Style, format: &'static str) -> Style {
        let key = (base.identity(), format);
        self.temporal
            .entry(key)
            .or_insert_with(|| base.clone().with_number_format(format))
            .clone()
    }
    fn state(&mut self, id: usize) -> &mut SheetState {
        let index = self
            .sheets
            .iter()
            .position(|(other, _)| *other == id)
            .unwrap_or(0);
        &mut self.sheets[index].1
    }
    fn close(&mut self, id: usize) -> Result<()> {
        let state = std::mem::take(self.state(id));
        if state.closed {
            *self.state(id) = state;
            return Ok(());
        }
        let name = state.name.clone();
        let result = (|| {
            if !state.features.is_empty() || !state.merges.is_empty() {
                self.inner
                    .set_interleaved_tail(id, state.features.clone(), None, &state.merges)?;
            }
            self.inner.close_interleaved_sheet(id)
        })();
        *self.state(id) = SheetState {
            name: name.clone(),
            closed: true,
            ..Default::default()
        };
        result.map_err(|error| error.with_sheet(name))
    }
    fn close_all(&mut self) -> Result<()> {
        if self.sheets.is_empty() {
            return Err(Error::new(
                ErrorKind::NoVisibleSheet,
                "Add a worksheet before you finish",
            ));
        }
        let ids: Vec<usize> = self.sheets.iter().map(|(id, _)| *id).collect();
        for id in ids {
            self.close(id)?;
        }
        Ok(())
    }
    /// Finish every sheet and write the file given to [`Writer::create`].
    /// The file is replaced atomically.
    pub fn finish(mut self) -> Result<()> {
        let target = self.target.take().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidState,
                "This writer has no path; use finish_into",
            )
        })?;
        self.close_all()?;
        let inner = self.inner;
        crabxl_xlsx::write_atomically(target, |file| inner.finish(file).map(|_| ()))
    }
    /// Finish every sheet and write the workbook to `output`.
    pub fn finish_into<W: Write + Seek>(mut self, output: W) -> Result<W> {
        self.close_all()?;
        self.inner.finish(output)
    }
}

/// Writes one worksheet of a [`Writer`].
pub struct WorksheetWriter<'a> {
    writer: &'a mut Writer,
    id: usize,
}

impl WorksheetWriter<'_> {
    fn ensure_no_rows(&mut self, what: &str) -> Result<()> {
        if self.writer.state(self.id).rows_written {
            return Err(Error::new(
                ErrorKind::InvalidState,
                format!("Set {what} before the first row"),
            ));
        }
        Ok(())
    }
    /// Append one row.
    pub fn append_row(&mut self, row: impl IntoRow) -> Result<u32> {
        self.write(row, None)
    }
    /// Append one row and give every cell of it a style.
    pub fn append_row_styled(&mut self, row: impl IntoRow, style: &Style) -> Result<u32> {
        let id = self.writer.style_id(style)?;
        self.write(row, Some((id, style)))
    }
    /// Append rows.
    pub fn append_rows<R: IntoRow>(&mut self, rows: impl IntoIterator<Item = R>) -> Result<()> {
        for row in rows {
            self.write(row, None)?;
        }
        Ok(())
    }
    fn write(&mut self, row: impl IntoRow, style: Option<(StyleId, &Style)>) -> Result<u32> {
        let index = self.writer.state(self.id).next_row;
        let mut cells = Vec::new();
        for (column, input) in row.into_cells().into_iter().enumerate() {
            let address = CellRef::new(index, column as u32)?.0;
            let style = match (&input, style) {
                (CellInput::Value(Value::Temporal(temporal)), style) => {
                    let base = match style {
                        Some((_, base)) => base.clone(),
                        None => Style::new(),
                    };
                    if base.number_format() == "General" {
                        let dated = self.writer.temporal_style(&base, temporal.default_format());
                        self.writer.style_id(&dated)?
                    } else {
                        style.map_or(StyleId::new(0), |(id, _)| id)
                    }
                }
                (_, Some((id, _))) => id,
                (_, None) => StyleId::new(0),
            };
            cells.push(Cell {
                address,
                value: to_core(input).map_err(|error| error.with_cell(address))?,
                style,
            });
        }
        self.writer.inner.activate_sheet(self.id)?;
        self.writer.inner.write_row(&crabxl_core::Row {
            index: RowIndex::new(index)?,
            cells,
        })?;
        let state = self.writer.state(self.id);
        state.next_row = index + 1;
        state.rows_written = true;
        Ok(index)
    }
    /// Leave rows empty before the next appended row.
    pub fn skip_rows(&mut self, count: u32) -> Result<()> {
        let state = self.writer.state(self.id);
        state.next_row = state
            .next_row
            .checked_add(count)
            .filter(|row| *row < crabxl_core::MAX_ROWS)
            .ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "Past the last row of the worksheet")
            })?;
        Ok(())
    }
    /// Set column widths, in characters of the default font.
    pub fn set_column_width(&mut self, columns: impl IntoSpan, width: f64) -> Result<()> {
        let span = columns.into_column_span()?;
        let mut column = ColumnDimension::new(
            ColumnIndex::new(span.start)?,
            ColumnIndex::new(span.start + span.count - 1)?,
        )?;
        column.width = Some(width);
        column.custom_width = Some(true);
        self.writer
            .inner
            .set_interleaved_column_dimension(self.id, column)
    }
    /// Freeze the rows above and the columns left of a cell. Call this
    /// before the first row.
    pub fn freeze_panes(&mut self, at: impl IntoCellRef) -> Result<()> {
        self.ensure_no_rows("frozen panes")?;
        let at = at.into_cell_ref()?;
        let position = match (at.row() > 0, at.column() > 0) {
            (true, true) => crabxl_core::PanePosition::BottomRight,
            (true, false) => crabxl_core::PanePosition::BottomLeft,
            _ => crabxl_core::PanePosition::TopRight,
        };
        let view = crabxl_core::SheetView {
            pane: Some(Box::new(crabxl_core::Pane {
                x_split: (at.column() > 0).then(|| f64::from(at.column())),
                y_split: (at.row() > 0).then(|| f64::from(at.row())),
                top_left_cell: Some(at.to_string().into()),
                active_pane: position,
                state: crabxl_core::PaneState::Frozen,
            })),
            ..Default::default()
        };
        let views = crabxl_core::SheetViews { views: vec![view] };
        self.writer
            .inner
            .set_interleaved_header(self.id, Some(&views), None)
    }
    /// Merge a range. Merges are written when the sheet finishes.
    pub fn merge(&mut self, range: impl IntoRange) -> Result<()> {
        let range = range.into_range()?;
        if range.is_cell() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Merge at least two cells",
            ));
        }
        self.writer.state(self.id).merges.push(range.0);
        Ok(())
    }
    /// Add a data validation rule.
    pub fn add_data_validation(&mut self, validation: DataValidation) -> Result<()> {
        if validation.inner.ranges.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Give the validation a range with with_range",
            ));
        }
        self.writer
            .state(self.id)
            .features
            .data_validations
            .validations
            .push(validation.inner);
        Ok(())
    }
    /// Add a conditional format.
    pub fn add_conditional_format(&mut self, format: ConditionalFormat) -> Result<()> {
        let mut rule = format.rule;
        if let Some(style) = format.style {
            rule.dxf_id = Some(self.writer.inner.register_differential_style(style.inner)?);
        }
        let features = &mut self.writer.state(self.id).features;
        rule.priority = Some(features.conditional_formats.len() as u32 + 1);
        features
            .conditional_formats
            .push(crabxl_core::features::ConditionalFormatting {
                ranges: format.ranges.iter().map(|range| range.0).collect(),
                pivot: None,
                rules: vec![rule],
            });
        Ok(())
    }
    /// Finish the sheet. Unfinished sheets finish with the writer.
    pub fn finish(self) -> Result<()> {
        self.writer.close(self.id)
    }
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer")
            .field(
                "sheets",
                &self
                    .sheets
                    .iter()
                    .map(|(_, s)| s.name.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl std::fmt::Debug for WorksheetWriter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorksheetWriter")
            .field("id", &self.id)
            .finish()
    }
}
