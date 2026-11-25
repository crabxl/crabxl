//! Read, write and edit Excel XLSX files.
//!
//! CrabXL has three entry points, one for each way of working with a file:
//!
//! | Type | Use it to | Memory |
//! |---|---|---|
//! | [`Workbook`] | create a workbook, or open one and change it | the sheets you use |
//! | [`read::Reader`] | read large files row by row | one row |
//! | [`write::Writer`] | write large files row by row | one row |
//!
//! All three share the same values, styles, references and errors.
//!
//! # Create a workbook
//!
//! ```
//! use crabxl::style::{Font, Style};
//! use crabxl::{Formula, Workbook};
//! # fn main() -> crabxl::Result<()> {
//! let mut book = Workbook::new()?;
//! let mut sheet = book.add_worksheet("Sales")?;
//! sheet.append_row(("Region", "Q1", "Q2", "Total"))?;
//! sheet.append_row(("North", 120.5, 98.0, Formula::new("B2+C2")?))?;
//! sheet.set_style("A1:D1", &Style::new().with_font(Font::new().with_bold(true)))?;
//! sheet.freeze_panes("A2")?;
//! let bytes = book.write_to(std::io::Cursor::new(Vec::new()), &Default::default())?;
//! # assert!(!bytes.into_inner().is_empty());
//! # Ok(())
//! # }
//! ```
//!
//! # Edit a file
//!
//! A save copies every part you did not change, and it fails instead of
//! dropping content that CrabXL cannot keep. Saves to a path are atomic.
//!
//! ```no_run
//! use crabxl::Workbook;
//! # fn main() -> crabxl::Result<()> {
//! let mut book = Workbook::open("report.xlsx")?;
//! let mut sheet = book.worksheet("Summary")?;
//! let total: f64 = (&sheet.value("B10")?).try_into()?;
//! sheet.insert_rows("5:6")?; // Formulas, names and ranges follow the cells.
//! sheet.set_value("B5", total * 2.0)?;
//! book.save("report.xlsx")?;
//! # Ok(())
//! # }
//! ```
//!
//! # Values
//!
//! A cell holds a [`Value`] and, optionally, a [`Formula`]. For a formula
//! cell the value is the cached result. Values never turn into formulas:
//! text that starts with `=` stays text. Numbers keep their full precision;
//! see [`Number`].

mod reference;
mod row;
mod value;
mod workbook;
mod worksheet;

pub mod features;
pub mod read;
pub mod style;
pub mod write;

pub use crabxl_core::{Error, ErrorKind, Result};
pub use crabxl_xlsx::DataLossPolicy;
/// The number of threads large reads, saves and compression may use, and a
/// way to lower it. `CRABXL_MAX_THREADS` sets the same limit.
pub use crabxl_xlsx::{max_threads, set_max_threads};
pub use reference::{CellRef, IntoCellRef, IntoRange, IntoSpan, RangeRef, Span};
pub use row::{FromRow, Grid, IntoRow};
pub use value::{
    CellInput, ErrorValue, Formula, FromValue, IntoCell, Number, RichText, Temporal, TemporalKind,
    Text, TextRun, Value,
};
pub use workbook::{
    Limits, Memory, OpenOptions, Properties, SaveOptions, Source, Visibility, Workbook,
};
pub use worksheet::{Cell, Worksheet};

#[cfg(feature = "calc")]
pub use crabxl_calc::CalcStats;
