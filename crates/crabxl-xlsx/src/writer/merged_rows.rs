//! Sparse merged-cell traversal without materializing virtual cell areas.
use super::*;

fn merged_row_bits(range: crabxl_core::CellRange, row: RowIndex) -> usize {
    (usize::from(row == range.start.row) << 2) | (usize::from(row == range.end.row) << 3)
}
fn merged_row_has_style(merge: &crabxl_core::MergedCellRange, row: RowIndex) -> bool {
    let range = merge.range();
    let bits = merged_row_bits(range, row);
    let styles = merge.appearances();
    if range.start.column == range.end.column {
        styles[bits | 3].get() != 0
    } else {
        styles[bits | 1].get() != 0
            || styles[bits | 2].get() != 0
            || range.end.column.get() - range.start.column.get() > 1 && styles[bits].get() != 0
    }
}
pub(crate) fn next_merged_row(sheet: &crabxl_core::Worksheet, start: u32) -> Option<RowIndex> {
    let mut best: Option<RowIndex> = None;
    for merge in sheet.merged_ranges().styled_ranges_from(start) {
        let range = merge.range();
        if best.is_some_and(|best| range.start.row >= best) {
            break;
        }
        let mut row = RowIndex::new(start.max(range.start.row.get())).ok()?;
        let candidate = if merged_row_has_style(merge, row) {
            Some(row)
        } else {
            if row == range.start.row && row < range.end.row {
                row = RowIndex::new(row.get() + 1).ok()?;
            }
            if merged_row_has_style(merge, row) {
                Some(row)
            } else {
                (row < range.end.row && merged_row_has_style(merge, range.end.row))
                    .then_some(range.end.row)
            }
        };
        if let Some(candidate) = candidate {
            best = Some(best.map_or(candidate, |best| best.min(candidate)));
        }
        if best.is_some_and(|best| best.get() == start) {
            break;
        }
    }
    best
}

#[derive(Clone)]
pub(crate) struct MergedRowCells<'a> {
    sheet: &'a crabxl_core::Worksheet,
    row: RowIndex,
    column: u32,
}
impl<'a> MergedRowCells<'a> {
    pub(crate) fn new(sheet: &'a crabxl_core::Worksheet, row: RowIndex) -> Self {
        Self {
            sheet,
            row,
            column: 0,
        }
    }
}
impl<'a> Iterator for MergedRowCells<'a> {
    type Item = CellView<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.column < MAX_COLUMNS {
            let column = crabxl_core::ColumnIndex::new(self.column).ok()?;
            let physical = self.sheet.row_cells_from(self.row, column).next();
            let mut next = physical.map(|cell| cell.address.column.get());
            for merge in self.sheet.merged_ranges().styled_ranges_at(self.row.get()) {
                let range = merge.range();
                if self.row < range.start.row
                    || self.row > range.end.row
                    || self.column > range.end.column.get()
                {
                    continue;
                }
                let bits = merged_row_bits(range, self.row);
                let left = range.start.column.get();
                let right = range.end.column.get();
                let appearances = merge.appearances();
                let mut include = |column: u32, style: StyleId| {
                    if column >= self.column && style.get() != 0 {
                        next = Some(next.map_or(column, |next| next.min(column)));
                    }
                };
                include(left, appearances[bits | if left == right { 3 } else { 1 }]);
                include(right, appearances[bits | if left == right { 3 } else { 2 }]);
                if right - left > 1 && self.column < right {
                    include(self.column.max(left + 1), appearances[bits]);
                }
            }
            let next = next?;
            self.column = next + 1;
            if let Some(physical) = physical
                && physical.address.column.get() == next
            {
                return Some(CellView::from(physical));
            }
            let address = crabxl_core::CellAddress::new(self.row.get(), next).ok()?;
            if let Some(style) = self.sheet.merged_ranges().virtual_style(address)
                && style.get() != 0
            {
                return Some(CellView {
                    address,
                    value: &crabxl_core::CellValue::Empty,
                    style,
                });
            }
        }
        None
    }
}
