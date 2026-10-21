//! Sparse realized-cell relocation; raw merge declarations retain their bounds.
use super::*;

impl Worksheet {
    pub(super) fn move_virtual_cells(
        &mut self,
        source: CellRange,
        destination: CellRange,
        rows: i32,
        columns: i32,
        copy: bool,
        scratch: usize,
    ) -> Result<()> {
        if self.hyperlinks.is_empty()
            && !self.merges.virtual_ranges().any(|entry| {
                entry.range().intersects(source) || entry.range().intersects(destination)
            })
        {
            return Ok(());
        }
        self.edit_virtual_cells(
            |area| {
                let mut pieces = [None; 17];
                let mut count = 0;
                let retained = if copy {
                    [Some(area), None, None, None]
                } else {
                    area.difference(source)
                };
                for piece in retained.into_iter().flatten() {
                    for piece in piece.difference(destination).into_iter().flatten() {
                        pieces[count] = Some((piece, 0, 0));
                        count += 1;
                    }
                }
                if let Some(piece) = area.intersection(source) {
                    pieces[count] = Some((piece, i64::from(rows), i64::from(columns)));
                }
                pieces
            },
            scratch,
        )
    }

    pub(super) fn shift_virtual_cells(
        &mut self,
        at: u32,
        count: u32,
        rows: bool,
        insert: bool,
    ) -> Result<()> {
        let maximum = if rows { MAX_ROWS } else { MAX_COLUMNS };
        let axis = |first| -> Result<CellRange> {
            CellRange::new(
                CellAddress::new(if rows { first } else { 0 }, if rows { 0 } else { first })?,
                CellAddress::new(MAX_ROWS - 1, MAX_COLUMNS - 1)?,
            )
        };
        let affected = axis(at)?;
        if self.hyperlinks.is_empty()
            && !self
                .merges
                .virtual_ranges()
                .any(|entry| entry.range().intersects(affected))
        {
            return Ok(());
        }
        let moving = if insert {
            Some(affected)
        } else if at + count < maximum {
            Some(axis(at + count)?)
        } else {
            None
        };
        let offset = i64::from(count) * if insert { 1 } else { -1 };
        self.edit_virtual_cells(
            |area| {
                let mut pieces = [None; 17];
                let mut count = 0;
                for piece in area.difference(affected).into_iter().flatten() {
                    pieces[count] = Some((piece, 0, 0));
                    count += 1;
                }
                if let Some(piece) = moving.and_then(|moving| area.intersection(moving)) {
                    pieces[count] = Some((
                        piece,
                        if rows { offset } else { 0 },
                        if rows { 0 } else { offset },
                    ));
                }
                pieces
            },
            0,
        )
    }

    fn edit_virtual_cells(
        &mut self,
        plan: impl Fn(CellRange) -> [Option<(CellRange, i64, i64)>; 17],
        scratch: usize,
    ) -> Result<()> {
        let links = if self.hyperlinks.is_empty() {
            None
        } else {
            let other = self.charged.saturating_sub(self.hyperlinks.heap_bytes());
            Some(
                self.hyperlinks.transformed(
                    &plan,
                    self.limits
                        .max_bytes
                        .saturating_sub(other)
                        .saturating_sub(self.cell_work_bytes())
                        .saturating_sub(scratch),
                )?,
            )
        };
        let other = self.charged.saturating_sub(self.merges.heap_bytes());
        let maximum = self
            .limits
            .max_bytes
            .saturating_sub(other)
            .saturating_sub(self.cell_work_bytes())
            .saturating_sub(scratch);
        self.merges.transform_virtual(
            plan,
            maximum.saturating_sub(links.as_ref().map_or(0, crate::Hyperlinks::heap_bytes)),
        )?;
        self.charged = other.saturating_add(self.merges.heap_bytes());
        if let Some(links) = links {
            self.charged = self
                .charged
                .saturating_sub(self.hyperlinks.heap_bytes())
                .saturating_add(links.heap_bytes());
            self.hyperlinks = links;
        }
        Ok(())
    }
}
