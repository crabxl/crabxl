// SPDX-License-Identifier: MIT
// Sparse range collection layout adapted from umya-spreadsheet.
// Copyright (c) 2020 MathNya. Provenance: third_party/ports.json.
//! Compact merged-cell geometry with shared virtual appearance IDs.

use crate::{CellAddress, CellRange, Error, ErrorKind, Result, StyleId};
mod mutation;

/// A finite merged rectangle and its shared edge/interior appearances.
/// No covered cell objects are allocated. The owning coordinator prepares
/// styles before registering geometry and retains the anchor's physical value.
#[derive(Clone, Debug)]
pub struct MergedCellRange {
    range: CellRange,
    appearances: [StyleId; 16],
    virtual_range: Option<CellRange>,
    identity: u64,
}

impl PartialEq for MergedCellRange {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range
            && self.appearances == other.appearances
            && self.virtual_range == other.virtual_range
    }
}
impl Eq for MergedCellRange {}

impl MergedCellRange {
    /// Construct validated geometry with already resolved workbook-local IDs.
    /// Appearance slots use left=1, right=2, top=4 and bottom=8 edge bits.
    pub fn new(range: CellRange, appearances: [StyleId; 16]) -> Result<Self> {
        CellRange::new(range.start, range.end)?;
        Ok(Self {
            range,
            appearances,
            virtual_range: Some(range),
            identity: 0,
        })
    }

    /// Inclusive merged geometry.
    pub const fn range(&self) -> CellRange {
        self.range
    }

    /// Shared edge/interior appearance IDs, including the no-edge slot.
    pub const fn appearances(&self) -> &[StyleId; 16] {
        &self.appearances
    }

    /// Original virtual appearance geometry; raw declarations have none.
    pub const fn appearance_range(&self) -> Option<CellRange> {
        self.virtual_range
    }
    /// Stable declaration identity within this merge collection.
    pub const fn identity(&self) -> u64 {
        self.identity
    }
    /// Resolve a covered non-anchor coordinate without materializing it.
    pub fn virtual_style(&self, address: CellAddress) -> Option<StyleId> {
        let original = self.virtual_range?;
        if address == original.start || !self.range.contains(address) {
            return None;
        }
        let mask = usize::from(address.column == original.start.column)
            | (usize::from(address.column == original.end.column) << 1)
            | (usize::from(address.row == original.start.row) << 2)
            | (usize::from(address.row == original.end.row) << 3);
        Some(self.appearances[mask])
    }
}

/// Bounded sparse merge declarations in registration order.
#[derive(Clone, Debug, Default)]
pub struct MergedRanges {
    ranges: Vec<MergedCellRange>,
    nodes: Vec<Node>,
    root: Option<u32>,
    detached: Vec<MergedCellRange>,
    next_identity: u64,
}

impl MergedRanges {
    /// Borrow canonical declarations without allocating covered coordinates.
    pub fn ranges(&self) -> &[MergedCellRange] {
        &self.ranges
    }

    /// Actual retained vector capacity, independent of rectangle area.
    pub fn heap_bytes(&self) -> usize {
        self.ranges
            .capacity()
            .saturating_mul(size_of::<MergedCellRange>())
            .saturating_add(self.nodes.capacity().saturating_mul(size_of::<Node>()))
            .saturating_add(
                self.detached
                    .capacity()
                    .saturating_mul(size_of::<MergedCellRange>()),
            )
    }

    /// Register geometry; a declaration contained in an existing range is reused.
    /// Failed reservations can retain capacity, which remains charged.
    pub fn insert_with_limit(&mut self, range: MergedCellRange, maximum: usize) -> Result<bool> {
        if !self.reserve_range(range.range, maximum)? {
            return Ok(false);
        }
        self.insert_reserved(range);
        Ok(true)
    }
    pub(crate) fn reserve_range(&mut self, range: CellRange, maximum: usize) -> Result<bool> {
        if self.contains(range) {
            return Ok(false);
        }
        if self.ranges.len() >= u32::MAX as usize || self.next_identity == u64::MAX {
            return Err(budget());
        }
        let retained = self.heap_bytes();
        if retained > maximum {
            return Err(budget());
        }
        if self.ranges.len() == self.ranges.capacity() {
            let growth = self.ranges.capacity().max(4);
            let additional = if retained
                .saturating_add(growth.saturating_mul(size_of::<MergedCellRange>()))
                <= maximum
            {
                growth
            } else {
                1
            };
            if retained.saturating_add(additional.saturating_mul(size_of::<MergedCellRange>()))
                > maximum
            {
                return Err(budget());
            }
            self.ranges.try_reserve_exact(additional).map_err(|cause| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot reserve merged geometry",
                    cause,
                )
            })?;
        }
        if self.nodes.len() == self.nodes.capacity() {
            let growth = self.nodes.capacity().max(4);
            let additional = if self
                .heap_bytes()
                .saturating_add(growth.saturating_mul(size_of::<Node>()))
                <= maximum
            {
                growth
            } else {
                1
            };
            if self
                .heap_bytes()
                .saturating_add(additional.saturating_mul(size_of::<Node>()))
                > maximum
            {
                return Err(budget());
            }
            self.nodes.try_reserve_exact(additional).map_err(|cause| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot reserve merged interval index",
                    cause,
                )
            })?;
        }
        if self.heap_bytes() > maximum {
            return Err(budget());
        }
        Ok(true)
    }
    pub(crate) fn insert_reserved(&mut self, mut range: MergedCellRange) {
        range.identity = self.next_identity;
        self.next_identity += 1;
        let index = self.ranges.len() as u32;
        self.ranges.push(range);
        self.nodes
            .push(Node::new(index, &self.ranges[index as usize]));
        self.root = Some(self.insert_node(self.root, index));
    }

    /// Remove an exact declaration without changing unrelated geometry.
    pub fn remove(&mut self, range: CellRange) -> Option<MergedCellRange> {
        let index = self
            .ranges
            .iter()
            .position(|existing| existing.range == range)?;
        let removed = self.ranges.remove(index);
        self.rebuild();
        Some(removed)
    }

    /// Resolve the most recently registered covering non-anchor appearance.
    pub fn virtual_style(&self, address: CellAddress) -> Option<StyleId> {
        let mut best = None;
        self.find_covering(self.root, address, None, &mut best);
        let mut selected = best.map(|index| &self.ranges[index as usize]);
        for range in &self.detached {
            if range.virtual_style(address).is_some()
                && selected.is_none_or(|previous| previous.identity < range.identity)
            {
                selected = Some(range);
            }
        }
        selected.and_then(|range| range.virtual_style(address))
    }
    /// Whether an existing declaration contains the complete finite rectangle.
    pub fn contains(&self, range: CellRange) -> bool {
        let mut best = None;
        self.find_covering(self.root, range.start, Some(range.end), &mut best);
        best.is_some()
    }
    /// Whether any covered coordinate carries a non-default virtual style.
    pub fn has_virtual_styles(&self) -> bool {
        self.root
            .is_some_and(|root| self.nodes[root as usize].style_end != 0)
            || self
                .detached
                .iter()
                .any(|range| range.appearances.iter().any(|style| style.get() != 0))
    }
    /// Borrow styled ranges covering a row, using the bounded interval index.
    pub fn styled_ranges_at(&self, row: u32) -> MergedRangeIter<'_> {
        MergedRangeIter::new(self, row, true)
    }
    /// Borrow styled ranges ending at or after a row, in start-row order.
    pub fn styled_ranges_from(&self, row: u32) -> MergedRangeIter<'_> {
        MergedRangeIter::new(self, row, false)
    }
    fn key(&self, index: u32) -> (u32, u32) {
        (self.ranges[index as usize].range.start.row.get(), index)
    }
    fn height(&self, index: Option<u32>) -> u8 {
        index.map_or(0, |index| self.nodes[index as usize].height)
    }
    fn update(&mut self, index: u32) {
        let mut node = self.nodes[index as usize];
        let range = &self.ranges[index as usize];
        node.max_end = range.range.end.row.get();
        node.max_id = index;
        node.style_end = if range.virtual_range.is_some()
            && range.appearances.iter().any(|style| style.get() != 0)
        {
            node.max_end + 1
        } else {
            0
        };
        node.height = 1 + self.height(node.left).max(self.height(node.right));
        for child in [node.left, node.right].into_iter().flatten() {
            let child = self.nodes[child as usize];
            node.max_end = node.max_end.max(child.max_end);
            node.max_id = node.max_id.max(child.max_id);
            node.style_end = node.style_end.max(child.style_end);
        }
        self.nodes[index as usize] = node;
    }
    fn rotate_left(&mut self, root: u32, pivot: u32) -> u32 {
        self.nodes[root as usize].right = self.nodes[pivot as usize].left;
        self.nodes[pivot as usize].left = Some(root);
        self.update(root);
        self.update(pivot);
        pivot
    }
    fn rotate_right(&mut self, root: u32, pivot: u32) -> u32 {
        self.nodes[root as usize].left = self.nodes[pivot as usize].right;
        self.nodes[pivot as usize].right = Some(root);
        self.update(root);
        self.update(pivot);
        pivot
    }
    fn insert_node(&mut self, root: Option<u32>, index: u32) -> u32 {
        let Some(root) = root else { return index };
        if self.key(index) < self.key(root) {
            self.nodes[root as usize].left =
                Some(self.insert_node(self.nodes[root as usize].left, index));
        } else {
            self.nodes[root as usize].right =
                Some(self.insert_node(self.nodes[root as usize].right, index));
        }
        self.update(root);
        let node = self.nodes[root as usize];
        let balance = i16::from(self.height(node.left)) - i16::from(self.height(node.right));
        if balance > 1
            && let Some(left) = node.left
        {
            if self.key(index) > self.key(left)
                && let Some(pivot) = self.nodes[left as usize].right
            {
                self.nodes[root as usize].left = Some(self.rotate_left(left, pivot));
            }
            if let Some(left) = self.nodes[root as usize].left {
                return self.rotate_right(root, left);
            }
        }
        if balance < -1
            && let Some(right) = node.right
        {
            if self.key(index) < self.key(right)
                && let Some(pivot) = self.nodes[right as usize].left
            {
                self.nodes[root as usize].right = Some(self.rotate_right(right, pivot));
            }
            if let Some(right) = self.nodes[root as usize].right {
                return self.rotate_left(root, right);
            }
        }
        root
    }
    fn find_covering(
        &self,
        root: Option<u32>,
        address: CellAddress,
        end: Option<CellAddress>,
        best: &mut Option<u32>,
    ) {
        let Some(root) = root else { return };
        let node = self.nodes[root as usize];
        if node.max_end < end.unwrap_or(address).row.get()
            || best.is_some_and(|best| node.max_id <= best)
        {
            return;
        }
        let range = &self.ranges[root as usize];
        if range.range.start.row > address.row {
            self.find_covering(node.left, address, end, best);
            return;
        }
        self.find_covering(node.right, address, end, best);
        if best.is_none_or(|best| root > best)
            && range.range.contains(address)
            && end.map_or(range.virtual_style(address).is_some(), |end| {
                range.range.contains(end)
            })
        {
            *best = Some(root);
        }
        self.find_covering(node.left, address, end, best);
    }
}

fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Merged geometry exceeds managed allowance",
    )
}

#[derive(Clone, Copy, Debug)]
struct Node {
    left: Option<u32>,
    right: Option<u32>,
    max_end: u32,
    max_id: u32,
    style_end: u32,
    height: u8,
}
impl Node {
    fn new(index: u32, range: &MergedCellRange) -> Self {
        let max_end = range.range.end.row.get();
        Self {
            left: None,
            right: None,
            max_end,
            max_id: index,
            style_end: if range.virtual_range.is_some()
                && range.appearances.iter().any(|style| style.get() != 0)
            {
                max_end + 1
            } else {
                0
            },
            height: 1,
        }
    }
}

/// Allocation-free traversal of indexed merged intervals.
pub struct MergedRangeIter<'a> {
    ranges: &'a MergedRanges,
    row: u32,
    exact: bool,
    stack: [u32; 64],
    len: usize,
    detached: usize,
}
impl<'a> MergedRangeIter<'a> {
    fn new(ranges: &'a MergedRanges, row: u32, exact: bool) -> Self {
        let mut result = Self {
            ranges,
            row,
            exact,
            stack: [0; 64],
            len: 0,
            detached: 0,
        };
        result.descend(ranges.root);
        result
    }
    fn descend(&mut self, mut index: Option<u32>) {
        while let Some(current) = index {
            let node = self.ranges.nodes[current as usize];
            if node.style_end <= self.row {
                return;
            }
            let range = &self.ranges.ranges[current as usize];
            if self.exact && range.range.start.row.get() > self.row {
                index = node.left;
                continue;
            }
            // An AVL tree with fewer than u32::MAX nodes has height below 64.
            self.stack[self.len] = current;
            self.len += 1;
            index = node.left;
        }
    }
}
impl<'a> Iterator for MergedRangeIter<'a> {
    type Item = &'a MergedCellRange;
    fn next(&mut self) -> Option<Self::Item> {
        while self.len > 0 {
            self.len -= 1;
            let current = self.stack[self.len];
            let node = self.ranges.nodes[current as usize];
            self.descend(node.right);
            let range = &self.ranges.ranges[current as usize];
            if range.virtual_range.is_some()
                && range.range.end.row.get() >= self.row
                && range.appearances.iter().any(|style| style.get() != 0)
            {
                return Some(range);
            }
        }
        while let Some(range) = self.ranges.detached.get(self.detached) {
            self.detached += 1;
            if range.range.end.row.get() >= self.row
                && (!self.exact || range.range.start.row.get() <= self.row)
                && range.appearances.iter().any(|style| style.get() != 0)
            {
                return Some(range);
            }
        }
        None
    }
}
