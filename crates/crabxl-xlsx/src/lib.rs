//! XLSX package discovery and bounded scalar worksheet streaming.

mod adaptive;
mod encode;
mod package;
mod reader;
mod styles;
mod writer;
mod xml;

pub use adaptive::{AdaptiveRead, ReadData};
pub use package::{SheetInfo, SheetKind, WorkbookReader};
pub use reader::Rows;

pub use writer::{WorkbookWriter, WriteOptions, WriteStats};

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
