//! Raw declaration edits preserve previously realized sparse virtual cells.
use super::*;

impl MergedRanges {
    /// Whether raw metadata edits retained independent appearance patterns.
    pub fn has_detached_appearances(&self) -> bool {
        !self.detached.is_empty()
    }

    pub(super) fn rebuild(&mut self) {
        self.nodes.clear();
        self.root = None;
        for index in 0..self.ranges.len() {
            let index = index as u32;
            self.nodes
                .push(Node::new(index, &self.ranges[index as usize]));
            self.root = Some(self.insert_node(self.root, index));
        }
    }
    /// Borrow a live declaration by stable identity, without coordinate lookups.
    pub fn declaration(&self, identity: u64) -> Option<&MergedCellRange> {
        self.ranges.iter().find(|range| range.identity == identity)
    }
    /// Visible cached geometry is independent of mutable serialized declarations.
    pub fn virtual_ranges(&self) -> impl Iterator<Item = &MergedCellRange> {
        self.ranges
            .iter()
            .filter(|range| range.virtual_range.is_some())
            .chain(self.detached.iter())
    }
    /// Add serialization-only metadata without creating covered virtual cells.
    pub fn add_declaration(&mut self, range: CellRange, maximum: usize) -> Result<bool> {
        CellRange::new(range.start, range.end)?;
        if !self.reserve_range(range, maximum)? {
            return Ok(false);
        }
        self.insert_reserved(MergedCellRange {
            range,
            appearances: [StyleId::new(0); 16],
            virtual_range: None,
            identity: 0,
        });
        Ok(true)
    }
    /// Remove an exact declaration while retaining existing virtual cell patterns.
    pub fn remove_declaration(&mut self, range: CellRange, maximum: usize) -> Result<bool> {
        let Some(index) = self
            .ranges
            .iter()
            .position(|existing| existing.range == range)
        else {
            return Ok(false);
        };
        self.reserve_detached(
            usize::from(self.ranges[index].virtual_range.is_some()),
            maximum,
        )?;
        let removed = self.ranges.remove(index);
        if removed.virtual_range.is_some() {
            self.detached.push(removed);
        }
        self.rebuild();
        Ok(true)
    }
    /// Atomically replace declaration geometry, preserving previously realized cells.
    pub fn replace_declaration(
        &mut self,
        identity: u64,
        range: CellRange,
        maximum: usize,
    ) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        let index = self
            .ranges
            .iter()
            .position(|existing| existing.identity == identity)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Unknown merged declaration identity",
                )
            })?;
        if self.ranges[index].range == range {
            return Ok(());
        }
        self.reserve_detached(
            usize::from(self.ranges[index].virtual_range.is_some()),
            maximum,
        )?;
        if self.ranges[index].virtual_range.is_some() {
            self.detached.push(self.ranges[index].clone());
        }
        self.ranges[index].virtual_range = None;
        self.ranges[index].range = range;
        self.rebuild();
        Ok(())
    }
    fn reserve_detached(&mut self, extra: usize, maximum: usize) -> Result<()> {
        let needed = self.detached.len().checked_add(extra).ok_or_else(budget)?;
        if needed > self.detached.capacity() {
            let additional = needed.saturating_sub(self.detached.capacity());
            if self
                .heap_bytes()
                .saturating_add(additional.saturating_mul(size_of::<MergedCellRange>()))
                > maximum
            {
                return Err(budget());
            }
            self.detached.try_reserve_exact(extra).map_err(|cause| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot reserve detached merged appearances",
                    cause,
                )
            })?;
        }
        if self.heap_bytes() > maximum {
            return Err(budget());
        }
        Ok(())
    }
    /// Clear virtual coverage for high-level unmerge without enumerating cells.
    /// Clipped pieces retain original edge geometry and registration identities.
    pub(crate) fn clear_virtual(&mut self, area: CellRange, maximum: usize) -> Result<()> {
        let fragments = self
            .virtual_ranges()
            .filter(|range| range.range.intersects(area))
            .map(|range| range.range.difference(area).into_iter().flatten().count())
            .sum::<usize>();
        self.reserve_detached(fragments, maximum)?;
        let original = self.detached.len();
        for index in (0..original).rev() {
            if !self.detached[index].range.intersects(area) {
                continue;
            }
            let previous = self.detached.swap_remove(index);
            for piece in previous.range.difference(area).into_iter().flatten() {
                let mut fragment = previous.clone();
                fragment.range = piece;
                self.detached.push(fragment);
            }
        }
        for previous in &mut self.ranges {
            if previous.virtual_range.is_none() || !previous.range.intersects(area) {
                continue;
            }
            for piece in previous.range.difference(area).into_iter().flatten() {
                let mut fragment = previous.clone();
                fragment.range = piece;
                self.detached.push(fragment);
            }
            previous.virtual_range = None;
        }
        self.rebuild();
        Ok(())
    }
}
