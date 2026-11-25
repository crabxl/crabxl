//! Streaming reads with bounded memory.
//!
//! A [`Reader`] decodes one worksheet row at a time. It never holds a whole
//! sheet, so it reads files of any size in a small, fixed amount of memory.
//!
//! ```no_run
//! use crabxl::read::{Reader, RowOptions};
//! # fn main() -> crabxl::Result<()> {
//! let mut reader = Reader::open("events.xlsx")?;
//! let options = RowOptions::new().with_columns("A:D")?.with_skip_rows(1);
//! let mut total = 0.0;
//! for row in reader.rows("Events", &options)? {
//!     let (_id, _kind, amount): (i64, String, f64) = row?.parse()?;
//!     total += amount;
//! }
//! # Ok(())
//! # }
//! ```

use crate::reference::{IntoSpan, Span};
use crate::row::FromRow;
use crate::value::{Formula, Value, from_core};
use crate::workbook::{OpenOptions, Source};
use crate::{Error, ErrorKind, Result};
use crabxl_core::{ColumnIndex, RowIndex};
use crabxl_xlsx::WorkbookReader;
use std::path::Path;

/// Which rows and columns [`Reader::rows`] returns.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct RowOptions {
    /// Only these columns. Each row then holds exactly these columns.
    pub columns: Option<Span>,
    /// Skip this many rows from the top of the sheet.
    pub skip_rows: u32,
    /// Stop after this many rows of the sheet (counted after the skipped rows).
    pub max_rows: Option<u32>,
}

impl RowOptions {
    /// Every row and column.
    pub fn new() -> Self {
        Self::default()
    }
    /// Read only these columns, such as `"A:D"`.
    pub fn with_columns(mut self, columns: impl IntoSpan) -> Result<Self> {
        self.columns = Some(columns.into_column_span()?);
        Ok(self)
    }
    /// Skip rows from the top, such as a header row.
    pub fn with_skip_rows(mut self, rows: u32) -> Self {
        self.skip_rows = rows;
        self
    }
    /// Stop after a number of rows.
    pub fn with_max_rows(mut self, rows: u32) -> Self {
        self.max_rows = Some(rows);
        self
    }
}

/// A forward-only reader for large workbooks.
pub struct Reader {
    inner: WorkbookReader<Box<dyn Source>>,
    exact_numbers: bool,
}

impl Reader {
    /// Open a file with the default options.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, &OpenOptions::default())
    }
    /// Open a file. The reader uses the limits and exact-number setting of
    /// `options`.
    pub fn open_with(path: impl AsRef<Path>, options: &OpenOptions) -> Result<Self> {
        let file = std::fs::File::open(path.as_ref()).map_err(|error| {
            Error::caused_by(
                ErrorKind::Io,
                format!("Cannot open {}", path.as_ref().display()),
                error,
            )
        })?;
        Self::from_reader(std::io::BufReader::new(file), options)
    }
    /// Read from any seekable byte source.
    pub fn from_reader(source: impl Source + 'static, options: &OpenOptions) -> Result<Self> {
        let inner = WorkbookReader::with_limits(
            Box::new(source) as Box<dyn Source>,
            options.limits.resources(),
        )?;
        Ok(Self {
            inner,
            exact_numbers: options.exact_numbers,
        })
    }
    /// The names of all sheets, in tab order.
    pub fn sheet_names(&self) -> Vec<String> {
        self.inner
            .sheets()
            .iter()
            .map(|sheet| sheet.name().to_owned())
            .collect()
    }
    /// The rows of a worksheet that hold at least one cell, in order.
    pub fn rows(&mut self, sheet: &str, options: &RowOptions) -> Result<Rows<'_>> {
        let name = self
            .inner
            .sheets()
            .iter()
            .find(|info| info.name().eq_ignore_ascii_case(sheet))
            .map(|info| info.name().to_owned())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    format!("No sheet named '{sheet}'"),
                )
            })?;
        let first = options.skip_rows;
        let last = match options.max_rows {
            Some(count) => first
                .saturating_add(count)
                .saturating_sub(1)
                .min(crabxl_core::MAX_ROWS - 1),
            None => crabxl_core::MAX_ROWS - 1,
        };
        let engine = crabxl_core::ReadOptions {
            rows: (options.skip_rows > 0 || options.max_rows.is_some())
                .then(|| -> Result<_> { Ok(RowIndex::new(first)?..=RowIndex::new(last)?) })
                .transpose()?,
            stop_after_last_row: options.max_rows.is_some(),
            columns: options
                .columns
                .map(|span| -> Result<_> {
                    Ok(ColumnIndex::new(span.start)?
                        ..=ColumnIndex::new(span.start + span.count - 1)?)
                })
                .transpose()?,
            exact_numbers: self.exact_numbers,
            rich_text: true,
            ..Default::default()
        };
        if options.max_rows == Some(0) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "max_rows must be at least 1",
            ));
        }
        let rows = self
            .inner
            .rows_with_options(&name, engine)
            .map_err(|error| error.with_sheet(&name))?;
        Ok(Rows {
            rows,
            columns: options.columns,
            buffer: crabxl_core::Row::new(RowIndex::new(0)?),
            sheet: name,
        })
    }
}

/// The rows of one worksheet. Each item is one row or the error that
/// stopped reading.
pub struct Rows<'a> {
    rows: crabxl_xlsx::Rows<'a, Box<dyn Source>>,
    columns: Option<Span>,
    buffer: crabxl_core::Row,
    sheet: String,
}

impl Rows<'_> {
    /// Read the next row into `row`, reusing its memory. Returns `false`
    /// after the last row.
    pub fn read_into(&mut self, row: &mut Row) -> Result<bool> {
        if !self
            .rows
            .read_row_into(&mut self.buffer)
            .map_err(|error| error.with_sheet(&self.sheet))?
        {
            return Ok(false);
        }
        row.index = self.buffer.index.get();
        row.values.clear();
        row.formulas.clear();
        let (first, width) = match self.columns {
            Some(span) => (span.start, span.count as usize),
            None => (
                0,
                self.buffer
                    .cells
                    .last()
                    .map_or(0, |cell| cell.address.column.get() as usize + 1),
            ),
        };
        row.first_column = first;
        row.values.resize(width, Value::Blank);
        for cell in self.buffer.cells.drain(..) {
            let column = (cell.address.column.get() - first) as usize;
            let (value, formula) = from_core(cell.value);
            if let Some(formula) = formula {
                if row.formulas.len() < width {
                    row.formulas.resize(width, None);
                }
                row.formulas[column] = Some(formula);
            }
            row.values[column] = value;
        }
        Ok(true)
    }
}

impl Iterator for Rows<'_> {
    type Item = Result<Row>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut row = Row::default();
        match self.read_into(&mut row) {
            Ok(true) => Some(Ok(row)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        }
    }
}

/// One row of values, dense from its first column.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    index: u32,
    first_column: u32,
    values: Vec<Value>,
    formulas: Vec<Option<Formula>>,
}

impl Row {
    /// An empty row, to reuse with [`Rows::read_into`].
    pub fn new() -> Self {
        Self::default()
    }
    /// The zero-based row index. Row 1 in Excel is index 0.
    pub fn index(&self) -> u32 {
        self.index
    }
    /// The zero-based column of the first value: the first selected column,
    /// or column A.
    pub fn first_column(&self) -> u32 {
        self.first_column
    }
    /// The values from the first column, with blanks for missing cells.
    pub fn values(&self) -> &[Value] {
        &self.values
    }
    /// The formula of the cell at a position in [`Row::values`], if any.
    pub fn formula(&self, position: usize) -> Option<&Formula> {
        self.formulas.get(position).and_then(Option::as_ref)
    }
    /// Convert the values, such as to `(String, f64, i64)`.
    pub fn parse<T: FromRow>(&self) -> Result<T> {
        T::from_row(&self.values).map_err(|error| {
            Error::new(
                error.kind(),
                format!("Row {}: {}", self.index + 1, error.message()),
            )
        })
    }
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("sheets", &self.sheet_names())
            .finish()
    }
}

impl std::fmt::Debug for Rows<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rows").field("sheet", &self.sheet).finish()
    }
}
