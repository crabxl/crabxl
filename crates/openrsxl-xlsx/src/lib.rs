//! XLSX package discovery and bounded numeric worksheet streaming.

mod adaptive;
mod package;
mod reader;
mod xml;

pub use adaptive::{AdaptiveRead, ReadData};
pub use package::{SheetInfo, SheetKind, WorkbookReader};
pub use reader::Rows;
