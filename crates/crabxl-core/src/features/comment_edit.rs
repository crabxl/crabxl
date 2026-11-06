use super::Comment;
use crate::{CellAddress, CellRange, Error, ErrorKind, Result};

/// Physical cell mutation applied to note positions. Other feature references
/// retain their original coordinates, matching openpyxl structural operations.
#[derive(Clone, Copy, Debug)]
pub enum CommentEdit {
    /// Insert or delete rows or columns.
    Shift {
        /// First zero-based index.
        at: u32,
        /// Number of rows or columns.
        count: u32,
        /// True for rows, false for columns.
        rows: bool,
        /// True for insertion, false for deletion.
        insert: bool,
    },
    /// Move or copy a rectangle, replacing notes at destination cells.
    Range {
        /// Source rectangle.
        range: CellRange,
        /// Row offset.
        rows: i32,
        /// Column offset.
        columns: i32,
        /// Copy instead of moving source notes.
        copy: bool,
    },
}
fn offset(address: CellAddress, rows: i32, columns: i32) -> Result<CellAddress> {
    let row = u32::try_from(i64::from(address.row.get()) + i64::from(rows)).map_err(|_| {
        Error::new(
            ErrorKind::InvalidData,
            "Comment movement exceeds worksheet bounds",
        )
    })?;
    let column =
        u32::try_from(i64::from(address.column.get()) + i64::from(columns)).map_err(|_| {
            Error::new(
                ErrorKind::InvalidData,
                "Comment movement exceeds worksheet bounds",
            )
        })?;
    CellAddress::new(row, column)
}
impl CommentEdit {
    /// Stage note edits without modifying the original collection.
    pub fn apply(self, comments: &[Comment]) -> Result<Vec<Comment>> {
        let mut output = Vec::with_capacity(comments.len());
        match self {
            Self::Shift {
                at,
                count,
                rows,
                insert,
            } => {
                let maximum = if rows {
                    crate::MAX_ROWS
                } else {
                    crate::MAX_COLUMNS
                };
                let end = at
                    .checked_add(count)
                    .filter(|end| *end <= maximum)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid comment shift"))?;
                if count == 0 {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Invalid comment shift count",
                    ));
                }
                for source in comments {
                    let index = if rows {
                        source.address.row.get()
                    } else {
                        source.address.column.get()
                    };
                    if !insert && (at..end).contains(&index) {
                        continue;
                    }
                    let index = if insert && index >= at {
                        index.checked_add(count).ok_or_else(|| {
                            Error::new(ErrorKind::InvalidData, "Comment index overflow")
                        })?
                    } else if !insert && index >= end {
                        index - count
                    } else {
                        index
                    };
                    let mut comment = source.clone();
                    comment.address = if rows {
                        CellAddress::new(index, source.address.column.get())?
                    } else {
                        CellAddress::new(source.address.row.get(), index)?
                    };
                    output.push(comment);
                }
            }
            Self::Range {
                range,
                rows,
                columns,
                copy,
            } => {
                let destination = CellRange::new(
                    offset(range.start, rows, columns)?,
                    offset(range.end, rows, columns)?,
                )?;
                for comment in comments {
                    if (copy || !range.contains(comment.address))
                        && !destination.contains(comment.address)
                    {
                        output.push(comment.clone());
                    }
                }
                for source in comments
                    .iter()
                    .filter(|comment| range.contains(comment.address))
                {
                    let mut comment = source.clone();
                    comment.address = offset(source.address, rows, columns)?;
                    output.push(comment);
                }
            }
        }
        Ok(output)
    }
}
