//! Relative view indexes and reference-compatible visibility normalization.
use crate::{Error, ErrorKind, Result, SheetVisibility};

/// Resolve a signed, possibly relative display index without changing its spelling.
/// Out-of-range indexes represent an unselected view.
pub fn resolve_sheet_index(index: i64, count: usize) -> Option<usize> {
    let count = count as i128;
    let index = if index < 0 {
        count + i128::from(index)
    } else {
        i128::from(index)
    };
    (0..count).contains(&index).then_some(index as usize)
}

/// Result of serializing a requested view index against sheet visibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveViewSelection {
    /// Active-tab attribute; None means omit it and retain the requested view.
    pub serialized_index: Option<i64>,
    /// Requested view after normalization, including relative or unselected indexes.
    pub requested_index: i64,
}

/// Normalize a deferred view without allocating a visible-sheet vector.
/// Visible targets retain the original signed index. Otherwise select from the
/// reference's sliced visible-sheet list; no following candidate omits activeTab.
pub fn normalize_active_view(
    index: i64,
    count: usize,
    visibility: impl Fn(usize) -> SheetVisibility,
) -> Result<ActiveViewSelection> {
    let mut visible =
        (0..count).filter(|position| visibility(*position) == SheetVisibility::Visible);
    let visible_count = visible.clone().count();
    if visible_count == 0 {
        return Err(Error::new(
            if count == 1 {
                ErrorKind::InvalidData
            } else {
                ErrorKind::NoVisibleSheet
            },
            if count == 1 {
                "The only worksheet cannot be hidden"
            } else {
                "At least one sheet must be visible"
            },
        ));
    }
    if resolve_sheet_index(index, count)
        .is_some_and(|position| visibility(position) == SheetVisibility::Visible)
    {
        return Ok(ActiveViewSelection {
            serialized_index: Some(index),
            requested_index: index,
        });
    }
    let offset = if index < 0 {
        (visible_count as i128 + i128::from(index)).max(0) as usize
    } else {
        usize::try_from(index)
            .unwrap_or(usize::MAX)
            .min(visible_count)
    };
    let selected = visible.nth(offset).map(|position| position as i64);
    Ok(ActiveViewSelection {
        serialized_index: selected,
        requested_index: selected.unwrap_or(index),
    })
}
