//! Cell comments (notes), modelled after openpyxl's `openpyxl/comments`.
use crate::{CellAddress, RichText};

/// Default comment box width in pixels (openpyxl default).
pub const DEFAULT_COMMENT_WIDTH: f64 = 144.0;
/// Default comment box height in pixels (openpyxl default).
pub const DEFAULT_COMMENT_HEIGHT: f64 = 79.0;

/// A cell comment (legacy note) with its author and box geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    /// Cell the comment is attached to.
    pub address: CellAddress,
    /// Comment author name.
    pub author: Box<str>,
    /// Plain comment text with all run formatting removed.
    pub text: Box<str>,
    /// Formatted runs, kept only when the comments part contained `<r>` runs.
    /// When present, `text` is their concatenation and writers emit the runs.
    pub rich_text: Option<RichText>,
    /// Box width in pixels (VML `width`).
    pub width: f64,
    /// Box height in pixels (VML `height`).
    pub height: f64,
    /// Whether the comment box is always shown (VML `visibility:visible`).
    pub visible: bool,
}
impl Default for Comment {
    fn default() -> Self {
        Self {
            address: CellAddress {
                row: crate::RowIndex::FIRST,
                column: crate::ColumnIndex::FIRST,
            },
            author: Box::default(),
            text: Box::default(),
            rich_text: None,
            width: DEFAULT_COMMENT_WIDTH,
            height: DEFAULT_COMMENT_HEIGHT,
            visible: false,
        }
    }
}
impl Comment {
    /// Create a hidden comment of default size.
    pub fn new(
        address: CellAddress,
        text: impl Into<Box<str>>,
        author: impl Into<Box<str>>,
    ) -> Self {
        Self {
            address,
            text: text.into(),
            author: author.into(),
            ..Self::default()
        }
    }
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.author.len()
            + self.text.len()
            + self.rich_text.as_ref().map_or(0, RichText::memory_bytes)
    }
}
