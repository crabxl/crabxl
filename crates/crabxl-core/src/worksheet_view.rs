// SPDX-License-Identifier: MIT
// Pane/selection composition selected from rust_xlsxwriter worksheet.rs,
// Copyright 2022-2026 John McNamara. Models and public-reference semantics
// are adapted to shared CrabXL ownership; see third_party/ports.json.
//! Worksheet display models without cell materialization or format-specific I/O.
use crate::{CellAddress, Error, ErrorKind, Result};

/// A visible pane's position relative to worksheet splits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanePosition {
    /// Upper left quadrant.
    TopLeft,
    /// Upper right quadrant.
    TopRight,
    /// Lower left quadrant.
    BottomLeft,
    /// Lower right quadrant.
    BottomRight,
}
impl PanePosition {
    /// OOXML/public descriptor spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "topLeft",
            Self::TopRight => "topRight",
            Self::BottomLeft => "bottomLeft",
            Self::BottomRight => "bottomRight",
        }
    }
}
/// Split or frozen viewport behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneState {
    /// Independent scrollable splits.
    Split,
    /// Frozen rows/columns.
    Frozen,
    /// Frozen split viewport.
    FrozenSplit,
}
impl PaneState {
    /// OOXML/public descriptor spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Split => "split",
            Self::Frozen => "frozen",
            Self::FrozenSplit => "frozenSplit",
        }
    }
}
/// Worksheet display mode, separate from printing configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// Ordinary cell grid.
    Normal,
    /// Page layout preview.
    PageLayout,
    /// Page-break preview.
    PageBreakPreview,
}
impl ViewMode {
    /// OOXML/public descriptor spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::PageLayout => "pageLayout",
            Self::PageBreakPreview => "pageBreakPreview",
        }
    }
}
/// Split/frozen viewport. Literal cell spelling is not eagerly interpreted as geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct Pane {
    /// Horizontal split, retaining fractional and negative finite descriptor values.
    pub x_split: Option<f64>,
    /// Vertical split, retaining fractional and negative finite descriptor values.
    pub y_split: Option<f64>,
    /// Literal scroll origin.
    pub top_left_cell: Option<Box<str>>,
    /// Focused pane.
    pub active_pane: PanePosition,
    /// Split/frozen state.
    pub state: PaneState,
}
impl Default for Pane {
    fn default() -> Self {
        Self {
            x_split: None,
            y_split: None,
            top_left_cell: None,
            active_pane: PanePosition::TopLeft,
            state: PaneState::Split,
        }
    }
}
impl Pane {
    /// Validate finite numeric values before model mutation/serialization.
    pub fn validate(&self) -> Result<()> {
        if self.x_split.is_some_and(|value| !value.is_finite())
            || self.y_split.is_some_and(|value| !value.is_finite())
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Pane splits must be finite",
            ));
        }
        Ok(())
    }
    /// Payload owned outside the fixed pane.
    pub fn heap_bytes(&self) -> usize {
        self.top_left_cell.as_ref().map_or(0, |value| value.len())
    }
}
/// One focused cell/range selection; missing properties remain distinct from defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    /// Optional pane containing this selection.
    pub pane: Option<PanePosition>,
    /// Literal active cell, including source spelling.
    pub active_cell: Option<Box<str>>,
    /// Optional active-cell index.
    pub active_cell_id: Option<i64>,
    /// Literal selected ranges, without dense coordinate expansion.
    pub ranges: Option<Box<str>>,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            pane: None,
            active_cell: Some("A1".into()),
            active_cell_id: None,
            ranges: Some("A1".into()),
        }
    }
}
impl Selection {
    /// Payload owned outside the fixed selection.
    pub fn heap_bytes(&self) -> usize {
        self.active_cell.as_ref().map_or(0, |value| value.len())
            + self.ranges.as_ref().map_or(0, |value| value.len())
    }
}
/// Complete baseline worksheet view attributes, panes and selections.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetView {
    /// Window protection indicator.
    pub window_protection: Option<bool>,
    /// Display formulas instead of their values.
    pub show_formulas: Option<bool>,
    /// Display grid lines.
    pub show_grid_lines: Option<bool>,
    /// Display row/column headers.
    pub show_row_column_headers: Option<bool>,
    /// Display zero values.
    pub show_zeros: Option<bool>,
    /// Right-to-left layout.
    pub right_to_left: Option<bool>,
    /// Selected worksheet tab.
    pub tab_selected: Option<bool>,
    /// Display ruler.
    pub show_ruler: Option<bool>,
    /// Display outline symbols.
    pub show_outline_symbols: Option<bool>,
    /// Use the default grid color.
    pub default_grid_color: Option<bool>,
    /// Display page whitespace.
    pub show_white_space: Option<bool>,
    /// Optional view mode; absence differs from explicit normal.
    pub mode: Option<ViewMode>,
    /// Literal viewport origin.
    pub top_left_cell: Option<Box<str>>,
    /// Grid color identity.
    pub color_id: Option<i64>,
    /// Current zoom percentage.
    pub zoom_scale: Option<i64>,
    /// Normal-view zoom percentage.
    pub zoom_scale_normal: Option<i64>,
    /// Sheet-layout zoom percentage.
    pub zoom_scale_sheet_layout: Option<i64>,
    /// Page-layout zoom percentage.
    pub zoom_scale_page_layout: Option<i64>,
    /// Zoom-to-fit indicator.
    pub zoom_to_fit: Option<bool>,
    /// Corresponding workbook view identity.
    pub workbook_view_id: i64,
    /// Optional split/frozen viewport, boxed to keep ordinary views smaller.
    pub pane: Option<Box<Pane>>,
    /// Source-ordered selections.
    pub selections: Vec<Selection>,
}
impl Default for SheetView {
    fn default() -> Self {
        Self {
            window_protection: None,
            show_formulas: None,
            show_grid_lines: None,
            show_row_column_headers: None,
            show_zeros: None,
            right_to_left: None,
            tab_selected: None,
            show_ruler: None,
            show_outline_symbols: None,
            default_grid_color: None,
            show_white_space: None,
            mode: None,
            top_left_cell: None,
            color_id: None,
            zoom_scale: None,
            zoom_scale_normal: None,
            zoom_scale_sheet_layout: None,
            zoom_scale_page_layout: None,
            zoom_to_fit: None,
            workbook_view_id: 0,
            pane: None,
            selections: vec![Selection::default()],
        }
    }
}
impl SheetView {
    /// Retained payload/capacities outside the fixed view.
    pub fn heap_bytes(&self) -> usize {
        self.top_left_cell.as_ref().map_or(0, |value| value.len())
            + self
                .pane
                .as_ref()
                .map_or(0, |pane| size_of::<Pane>() + pane.heap_bytes())
            + self.selections.capacity() * size_of::<Selection>()
            + self
                .selections
                .iter()
                .map(Selection::heap_bytes)
                .sum::<usize>()
    }
    /// Freeze before an address; A1/None clears the pane while retaining selections,
    /// matching public assignment behavior. Existing selection payloads are moved.
    pub fn freeze_at(&mut self, address: Option<CellAddress>) -> Result<()> {
        let Some(address) =
            address.filter(|address| address.row.get() != 0 || address.column.get() != 0)
        else {
            self.pane = None;
            return Ok(());
        };
        if self.selections.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidState,
                "Freezing panes requires a primary selection",
            ));
        }
        let row = address.row.get();
        let column = address.column.get();
        let both = row > 0 && column > 0;
        if both {
            self.selections.try_reserve_exact(2).map_err(|error| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot allocate pane selections",
                    error,
                )
            })?;
        }
        let position = if both {
            PanePosition::BottomRight
        } else if column > 0 {
            PanePosition::TopRight
        } else {
            PanePosition::BottomLeft
        };
        let pane = Box::new(Pane {
            x_split: (column > 0).then_some(f64::from(column)),
            y_split: (row > 0).then_some(f64::from(row)),
            top_left_cell: Some(address.to_string().into_boxed_str()),
            active_pane: position,
            state: PaneState::Frozen,
        });
        self.selections[0].pane = Some(position);
        if both {
            self.selections.insert(
                0,
                Selection {
                    pane: Some(PanePosition::TopRight),
                    active_cell: None,
                    active_cell_id: None,
                    ranges: None,
                },
            );
            self.selections.insert(
                1,
                Selection {
                    pane: Some(PanePosition::BottomLeft),
                    active_cell: None,
                    active_cell_id: None,
                    ranges: None,
                },
            );
        }
        self.pane = Some(pane);
        Ok(())
    }
}
/// Source-ordered worksheet views. No cells or dense ranges are retained here.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetViews {
    /// View records, including caller-explicit empty lists.
    pub views: Vec<SheetView>,
}
impl Default for SheetViews {
    fn default() -> Self {
        Self {
            views: vec![SheetView::default()],
        }
    }
}
impl SheetViews {
    /// Fixed wrapper, actual vector capacities and owned nested payloads.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.views.capacity() * size_of::<SheetView>()
            + self.views.iter().map(SheetView::heap_bytes).sum::<usize>()
    }
    /// Validate known model invariants without changing source properties.
    pub fn validate(&self) -> Result<()> {
        for view in &self.views {
            if let Some(pane) = &view.pane {
                pane.validate()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn public_freeze_quadrants_and_clear_retain_selections() {
        for (address, x, y, active, count) in [
            ("B1", Some(1.0), None, PanePosition::TopRight, 1),
            ("A3", None, Some(2.0), PanePosition::BottomLeft, 1),
            ("B3", Some(1.0), Some(2.0), PanePosition::BottomRight, 3),
        ] {
            let mut view = SheetView::default();
            view.freeze_at(Some(address.parse().unwrap())).unwrap();
            let pane = view.pane.as_ref().unwrap();
            assert_eq!(
                (pane.x_split, pane.y_split, pane.active_pane),
                (x, y, active)
            );
            assert_eq!(view.selections.len(), count);
            assert_eq!(
                view.selections[count - 1].active_cell.as_deref(),
                Some("A1")
            );
            if count == 3 {
                assert_eq!(view.selections[0].active_cell, None);
                assert_eq!(view.selections[1].ranges, None);
            }
            let selections = view.selections.clone();
            view.freeze_at(Some("A1".parse().unwrap())).unwrap();
            assert!(view.pane.is_none());
            assert_eq!(view.selections, selections);
        }
    }
    #[test]
    fn empty_selection_failure_does_not_mutate_and_nonfinite_splits_reject() {
        let mut view = SheetView {
            selections: Vec::new(),
            ..SheetView::default()
        };
        assert!(view.freeze_at(Some("B3".parse().unwrap())).is_err());
        assert!(view.pane.is_none());
        for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(
                Pane {
                    x_split: Some(value),
                    ..Pane::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Pane {
                x_split: Some(-1.5),
                ..Pane::default()
            }
            .validate()
            .is_ok()
        );
    }
}
