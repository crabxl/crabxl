//! Budgeted relocation of hyperlink owners independently of serialized refs.
use super::*;
use crate::CellRange;

impl Hyperlinks {
    pub(crate) fn transformed(
        &self,
        plan: &impl Fn(CellRange) -> [Option<(CellRange, i64, i64)>; 17],
        maximum: usize,
    ) -> Result<Self> {
        let mut prepared = Self::default();
        for (address, link) in self.iter() {
            let coverage = self.ranges.get(&(address.row, address.column)).copied();
            let area = coverage.unwrap_or(CellRange {
                start: address,
                end: address,
            });
            for (piece, rows, columns) in plan(area).into_iter().flatten() {
                let moved = piece.adjusted(columns, rows, columns, rows)?;
                let reference = if coverage.is_some() {
                    let original = link
                        .reference
                        .as_deref()
                        .map(str::parse::<CellRange>)
                        .transpose()?
                        .unwrap_or(area);
                    let row_offset =
                        i64::from(original.start.row.get()) - i64::from(area.start.row.get());
                    let column_offset =
                        i64::from(original.start.column.get()) - i64::from(area.start.column.get());
                    Some(
                        piece
                            .adjusted(column_offset, row_offset, column_offset, row_offset)?
                            .to_string(),
                    )
                } else if let Some(reference) = &link.reference {
                    Some(reference.to_string())
                } else if moved.start != address {
                    Some(address.to_string())
                } else {
                    None
                };
                let bytes = POINT_BYTES
                    .saturating_add(link.heap_bytes())
                    .saturating_sub(link.reference.as_ref().map_or(0, |value| value.len()))
                    .saturating_add(reference.as_ref().map_or(0, String::len))
                    .saturating_add(if moved.start != moved.end {
                        ranges::RANGE_BYTES
                    } else {
                        0
                    });
                if self
                    .bytes
                    .saturating_add(prepared.bytes)
                    .saturating_add(bytes)
                    > maximum
                {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Hyperlink structural work exceeds allowance",
                    ));
                }
                let mut value = link.clone();
                value.reference = reference.map(String::into_boxed_str);
                prepared.insert_structural(moved, value);
            }
        }
        Ok(prepared)
    }

    fn insert_structural(&mut self, area: CellRange, value: Hyperlink) {
        let coverage = (area.start != area.end).then_some(area);
        let owner = (area.start.row, area.start.column);
        self.bytes += POINT_BYTES
            + value.heap_bytes()
            + usize::from(coverage.is_some()) * ranges::RANGE_BYTES;
        self.references += usize::from(value.reference.is_some());
        if let Some(coverage) = coverage {
            self.ranges.insert(owner, coverage);
        }
        self.points.insert(owner, value);
    }

    /// Resolve a covered cell's public reference after sparse ownership moves.
    pub fn covered_reference(&self, address: CellAddress) -> Result<Option<String>> {
        let Some((owner, area)) = self.ranges.iter().find(|(_, area)| area.contains(address))
        else {
            return Ok(None);
        };
        let link = &self.points[owner];
        let original = link
            .reference
            .as_deref()
            .map(str::parse::<CellRange>)
            .transpose()?
            .unwrap_or(*area);
        let rows = i64::from(original.start.row.get()) - i64::from(area.start.row.get());
        let columns = i64::from(original.start.column.get()) - i64::from(area.start.column.get());
        let point = CellRange {
            start: address,
            end: address,
        }
        .adjusted(columns, rows, columns, rows)?;
        Ok(Some(point.start.to_string()))
    }
}
