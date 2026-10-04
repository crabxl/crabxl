//! XLSX package discovery and bounded numeric worksheet streaming.

mod package;
mod reader;
mod xml;

pub use package::{SheetInfo, SheetKind, WorkbookReader};
pub use reader::Rows;
