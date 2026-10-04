//! XLSX package discovery and bounded scalar worksheet streaming.

mod adaptive;
mod encode;
mod formula_codec;
mod package;
mod reader;
mod styles;
mod writer;
mod xml;

pub use adaptive::{AdaptiveRead, ReadData};
pub use package::{SheetInfo, SheetKind, WorkbookReader};
pub use reader::Rows;

pub use writer::{
    FormulaWritePolicy, NonFiniteWritePolicy, StyleWritePolicy, WorkbookWriter, WriteOptions,
    WriteStats,
};

mod editor;
pub use editor::{
    CalculationChainPolicy, EditorOptions, PartInfo, SaveOptions, SaveStats, WorkbookEditor,
};

pub use adaptive::memory_allowance;

mod strings;
pub use strings::{SharedStringOptions, SharedStringStats, SharedStringStorage};

mod rich_text;

mod formatting;

mod style_codec;

mod style_reader;

pub use formula_codec::SharedFormulaStats;

mod default_theme;
mod theme;
pub use theme::ThemeWritePolicy;

mod aggregate;
