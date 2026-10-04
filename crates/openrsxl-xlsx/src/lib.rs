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
pub use editor::{EditorOptions, PartInfo, SaveOptions, SaveStats, WorkbookEditor};

pub use adaptive::memory_allowance;
