//! Merges operations for the canonical Worksheet owner.
use super::*;

impl Worksheet {
    /// Borrow compact merged geometry without expanding covered coordinates.
    pub fn merged_ranges(&self) -> &crate::MergedRanges {
        &self.merges
    }
    /// Adopt prepared merge declarations without materializing covered cells.
    /// This low-level operation retains physical cells; the workbook coordinator
    /// owns anchor/border normalization and public merge mutation semantics.
    pub fn set_merged_ranges(&mut self, merges: crate::MergedRanges) -> Result<()> {
        let charged = self
            .charged
            .saturating_sub(self.merges.heap_bytes())
            .saturating_add(merges.heap_bytes());
        self.check(charged, self.len())?;
        self.merges = merges;
        self.charged = charged;
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn apply_merge(
        &mut self,
        range: crate::MergedCellRange,
        anchor_style: StyleId,
    ) -> Result<()> {
        self.guard_merge_hyperlinks(range.range())?;
        let other = self.charged.saturating_sub(self.merges.heap_bytes());
        let reserved = self
            .merges
            .reserve_range(range.range(), self.limits.max_bytes.saturating_sub(other));
        self.charged = other.saturating_add(self.merges.heap_bytes());
        if !reserved? {
            return Ok(());
        }
        let geometry = range.range();
        if let Some(anchor) = self.cells.get_mut(&key(geometry.start)) {
            anchor.style = anchor_style;
        } else {
            self.set(Cell {
                address: geometry.start,
                value: CellValue::Empty,
                style: anchor_style,
            })?;
        }
        self.merges.insert_reserved(range);
        self.remove_merge_interior(geometry);
        self.dirty = true;
        Ok(())
    }
    /// Remove an exact merge declaration and its physical non-anchor overrides.
    pub fn unmerge_cells(&mut self, range: CellRange) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        if self.merges.remove(range).is_none() {
            return Err(invalid("Cell range is not merged"));
        }
        self.remove_merge_interior(range);
        self.dirty = true;
        Ok(())
    }
    fn remove_merge_interior(&mut self, range: CellRange) {
        let storage = self.cells.storage_bytes();
        let mut removed_bytes = 0usize;
        self.cells.remove_where(
            key(range.start)..=key(range.end),
            |cell| cell.address != range.start && range.contains(cell.address),
            |cell| {
                removed_bytes = removed_bytes.saturating_add(charge(&cell));
            },
        );
        self.charged = self
            .charged
            .saturating_sub(storage)
            .saturating_sub(removed_bytes)
            .saturating_add(self.cells.storage_bytes());
    }
    /// Resolve shared appearance, including a virtual covered coordinate.
    pub fn style_at(&self, address: CellAddress) -> StyleId {
        self.get(address)
            .map(|cell| cell.style)
            .or_else(|| self.merges.virtual_style(address))
            .unwrap_or(StyleId::new(0))
    }
    /// Validate all physical, dimension and virtual appearances against one owner.
    pub fn validate_style_links(&self, catalog: Option<&crate::StyleCatalog>) -> Result<()> {
        let cells = self.cells().map(|cell| (cell.address, cell.style));
        let rows = self.dimensions.rows().iter().filter_map(|row| {
            row.style.map(|style| {
                (
                    CellAddress {
                        row: row.index,
                        column: ColumnIndex::FIRST,
                    },
                    style,
                )
            })
        });
        let columns = self.dimensions.columns().iter().filter_map(|column| {
            column.style.map(|style| {
                (
                    CellAddress {
                        row: RowIndex::FIRST,
                        column: column.start,
                    },
                    style,
                )
            })
        });
        let merges = self.merges.ranges().iter().flat_map(|range| {
            range
                .appearances()
                .iter()
                .map(move |style| (range.range().start, *style))
        });
        for (address, style) in cells.chain(rows).chain(columns).chain(merges) {
            if let Some(catalog) = catalog {
                catalog
                    .cell_style(style)
                    .map_err(|error| error.with_cell(address))?;
            } else if style.get() != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unknown worksheet appearance identity",
                )
                .with_cell(address));
            }
        }
        Ok(())
    }
}
