//! Raw declaration edits preserve previously realized sparse virtual cells.
use super::*;

impl MergedRanges {
    /// Transform realized sparse cells independently of serialized declarations.
    /// The planner returns bounded rectangle pieces and their translations.
    pub(crate) fn transform_virtual(
        &mut self,
        plan: impl Fn(CellRange) -> [Option<(CellRange, i64, i64)>; 17],
        maximum: usize,
    ) -> Result<()> {
        let count = self.virtual_ranges().try_fold(0usize, |total, entry| {
            let count = plan(entry.range).into_iter().flatten().count();
            total.checked_add(count).ok_or_else(budget)
        })?;
        let bytes = count
            .checked_mul(size_of::<MergedCellRange>())
            .ok_or_else(budget)?;
        if self.heap_bytes().saturating_add(bytes) > maximum {
            return Err(budget());
        }
        let mut prepared = Vec::new();
        prepared.try_reserve_exact(count).map_err(|_| budget())?;
        if self.heap_bytes().saturating_add(
            prepared
                .capacity()
                .saturating_mul(size_of::<MergedCellRange>()),
        ) > maximum
        {
            return Err(budget());
        }
        for entry in self.virtual_ranges() {
            let original = entry.virtual_range.ok_or_else(budget)?;
            for (piece, rows, columns) in plan(entry.range).into_iter().flatten() {
                let range = piece.adjusted(columns, rows, columns, rows)?;
                let mut appearances = [StyleId::new(0); 16];
                let edges = usize::from(piece.start.column == original.start.column)
                    | (usize::from(piece.end.column == original.end.column) << 1)
                    | (usize::from(piece.start.row == original.start.row) << 2)
                    | (usize::from(piece.end.row == original.end.row) << 3);
                for (mask, style) in appearances.iter_mut().enumerate() {
                    *style = entry.appearances[mask & edges];
                }
                let virtual_anchor = entry
                    .virtual_anchor
                    .filter(|anchor| piece.contains(*anchor))
                    .map(|anchor| {
                        CellRange {
                            start: anchor,
                            end: anchor,
                        }
                        .adjusted(columns, rows, columns, rows)
                        .map(|range| range.start)
                    })
                    .transpose()?;
                prepared.push(MergedCellRange {
                    range,
                    appearances,
                    virtual_range: Some(range),
                    virtual_anchor,
                    identity: entry.identity,
                });
            }
        }
        for entry in &mut self.ranges {
            entry.virtual_range = None;
            entry.virtual_anchor = None;
        }
        self.detached = prepared;
        self.rebuild();
        Ok(())
    }

    /// Atomically replace raw membership, retaining realized appearances and
    /// identities of unchanged declarations. Preparation is byte-budgeted
    /// together with the current collection; covered cells are never expanded.
    pub fn replace_declarations(&mut self, ranges: &[CellRange], maximum: usize) -> Result<()> {
        for range in ranges {
            CellRange::new(range.start, range.end)?;
        }
        let key = |range: CellRange| {
            (
                range.start.row,
                range.start.column,
                range.end.row,
                range.end.column,
            )
        };
        let scratch = ranges
            .len()
            .saturating_mul(size_of::<CellRange>())
            .saturating_add(self.ranges.len().saturating_mul(size_of::<usize>()));
        if self.heap_bytes().saturating_add(scratch) > maximum {
            return Err(budget());
        }
        let mut incoming = Vec::new();
        incoming
            .try_reserve_exact(ranges.len())
            .map_err(|_| budget())?;
        incoming.extend_from_slice(ranges);
        incoming.sort_unstable_by_key(|range| key(*range));
        incoming.dedup();
        let mut existing = Vec::new();
        existing
            .try_reserve_exact(self.ranges.len())
            .map_err(|_| budget())?;
        existing.extend(0..self.ranges.len());
        existing.sort_unstable_by_key(|index| key(self.ranges[*index].range));
        let scratch = incoming
            .capacity()
            .saturating_mul(size_of::<CellRange>())
            .saturating_add(existing.capacity().saturating_mul(size_of::<usize>()));
        let retained = |range: CellRange| {
            incoming
                .binary_search_by_key(&key(range), |entry| key(*entry))
                .is_ok()
        };
        let removed = self
            .ranges
            .iter()
            .filter(|entry| entry.virtual_range.is_some() && !retained(entry.range))
            .count();
        let detached = self
            .detached
            .len()
            .checked_add(removed)
            .ok_or_else(budget)?;
        let bytes = incoming
            .len()
            .checked_mul(size_of::<MergedCellRange>() + size_of::<Node>())
            .and_then(|bytes| {
                detached
                    .checked_mul(size_of::<MergedCellRange>())
                    .and_then(|extra| bytes.checked_add(extra))
            })
            .ok_or_else(budget)?;
        if self
            .heap_bytes()
            .saturating_add(scratch)
            .saturating_add(bytes)
            > maximum
        {
            return Err(budget());
        }
        let mut prepared = Self {
            next_identity: self.next_identity,
            ..Self::default()
        };
        prepared
            .ranges
            .try_reserve_exact(incoming.len())
            .map_err(|_| budget())?;
        prepared
            .nodes
            .try_reserve_exact(incoming.len())
            .map_err(|_| budget())?;
        prepared
            .detached
            .try_reserve_exact(detached)
            .map_err(|_| budget())?;
        if self
            .heap_bytes()
            .saturating_add(scratch)
            .saturating_add(prepared.heap_bytes())
            > maximum
        {
            return Err(budget());
        }
        prepared.detached.extend(self.detached.iter().cloned());
        prepared.detached.extend(
            self.ranges
                .iter()
                .filter(|entry| entry.virtual_range.is_some() && !retained(entry.range))
                .cloned(),
        );
        for range in &incoming {
            let entry = if let Ok(index) =
                existing.binary_search_by_key(&key(*range), |index| key(self.ranges[*index].range))
            {
                self.ranges[existing[index]].clone()
            } else {
                let identity = prepared.next_identity;
                prepared.next_identity = identity.checked_add(1).ok_or_else(budget)?;
                MergedCellRange {
                    range: *range,
                    appearances: [StyleId::new(0); 16],
                    virtual_range: None,
                    virtual_anchor: None,
                    identity,
                }
            };
            if prepared.ranges.len() >= u32::MAX as usize {
                return Err(budget());
            }
            prepared.ranges.push(entry);
        }
        prepared.ranges.sort_unstable_by_key(|entry| entry.identity);
        prepared.rebuild();
        *self = prepared;
        Ok(())
    }

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
        self.ranges
            .binary_search_by_key(&identity, |entry| entry.identity)
            .ok()
            .map(|index| &self.ranges[index])
    }
    /// Visible cached geometry is independent of mutable serialized declarations.
    pub fn virtual_ranges(&self) -> impl Iterator<Item = &MergedCellRange> {
        self.ranges
            .iter()
            .filter(|range| range.virtual_range.is_some())
            .chain(self.detached.iter())
    }
    /// Add serialization-only metadata without creating covered virtual cells.
    pub fn add_exact_declaration(&mut self, range: CellRange, maximum: usize) -> Result<bool> {
        CellRange::new(range.start, range.end)?;
        if self.contains(range) && self.ranges.iter().any(|entry| entry.range == range) {
            return Ok(false);
        }
        self.reserve_entry(maximum)?;
        self.insert_reserved(MergedCellRange {
            range,
            appearances: [StyleId::new(0); 16],
            virtual_range: None,
            virtual_anchor: None,
            identity: 0,
        });
        Ok(true)
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
            virtual_anchor: None,
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
