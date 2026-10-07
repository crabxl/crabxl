//! Ordered sparse cells in bounded contiguous blocks, without per-cell tree nodes.
use crate::Cell;
use std::{
    collections::{BTreeMap, btree_map},
    ops::RangeInclusive,
    vec,
};

type Key = (u32, u32);
const BLOCK_CELLS: usize = 128;
const BLOCK_BYTES: usize = 512;

#[derive(Default)]
pub(crate) struct CellStore {
    blocks: BTreeMap<Key, Vec<Cell>>,
    len: usize,
    last: Option<Key>,
    slots: usize,
    tail_len: usize,
    tail_capacity: usize,
    tail_first: Option<Key>,
}
fn key(cell: &Cell) -> Key {
    (cell.address.row.get(), cell.address.column.get())
}
impl CellStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub(crate) fn storage_bytes(&self) -> usize {
        self.slots
            .saturating_mul(size_of::<Cell>())
            .saturating_add(self.blocks.len().saturating_mul(BLOCK_BYTES))
    }
    #[inline(always)]
    pub(crate) fn insertion_growth(&self, target: Key) -> (usize, usize) {
        if self.last.is_none_or(|last| target > last) {
            return growth(self.tail_len, self.tail_capacity);
        }
        let Some((_, block)) = self.blocks.range(..=target).next_back() else {
            return self
                .blocks
                .first_key_value()
                .map_or(growth(0, 0), |(_, block)| {
                    growth(block.len(), block.capacity())
                });
        };
        match block.binary_search_by_key(&target, key) {
            Ok(_) => (0, 0),
            Err(index) if block.len() == BLOCK_CELLS => {
                if index == block.len() {
                    growth(0, 0)
                } else {
                    let capacity = if index < BLOCK_CELLS / 2 {
                        BLOCK_CELLS / 2
                    } else {
                        BLOCK_CELLS
                    };
                    let bytes = capacity * size_of::<Cell>();
                    (bytes + BLOCK_BYTES, bytes + BLOCK_BYTES)
                }
            }
            Err(_) => growth(block.len(), block.capacity()),
        }
    }
    pub(crate) fn append_growth(&self, count: usize) -> (usize, usize) {
        let (mut len, mut capacity) = (self.tail_len, self.tail_capacity);
        let mut retained = 0usize;
        let mut peak = 0usize;
        for _ in 0..count {
            let (delta, work) = growth(len, capacity);
            peak = peak.max(retained.saturating_add(work));
            retained = retained.saturating_add(delta);
            if len == 0 || len == BLOCK_CELLS {
                len = 1;
                capacity = 1;
            } else {
                if len == capacity {
                    capacity = next_capacity(capacity);
                }
                len += 1;
            }
        }
        (retained, peak)
    }
    pub(crate) fn values(&self) -> impl DoubleEndedIterator<Item = &Cell> + Clone {
        self.blocks.values().flat_map(|block| block.iter())
    }
    pub(crate) fn range(
        &self,
        range: RangeInclusive<Key>,
    ) -> impl Iterator<Item = (Key, &Cell)> + Clone {
        let start = *range.start();
        let end = *range.end();
        let first = self
            .blocks
            .range(..=start)
            .next_back()
            .map_or(start, |(key, _)| *key);
        self.blocks
            .range(first..=end)
            .flat_map(move |(_, block)| {
                let from = block.partition_point(|cell| key(cell) < start);
                let to = block.partition_point(|cell| key(cell) <= end);
                // Reversed bounds yield an empty slice, as the prior filter did.
                block[from.min(to)..to].iter()
            })
            .map(|cell| (key(cell), cell))
    }
    pub(crate) fn get(&self, target: &Key) -> Option<&Cell> {
        if self.last.is_none_or(|last| *target > last) {
            return None;
        }
        let (_, block) = self.blocks.range(..=*target).next_back()?;
        block
            .binary_search_by_key(target, key)
            .ok()
            .map(|index| &block[index])
    }
    pub(crate) fn get_mut(&mut self, target: &Key) -> Option<&mut Cell> {
        if self.last.is_none_or(|last| *target > last) {
            return None;
        }
        let (_, block) = self.blocks.range_mut(..=*target).next_back()?;
        block
            .binary_search_by_key(target, key)
            .ok()
            .map(|index| &mut block[index])
    }
    pub(crate) fn insert(&mut self, target: Key, cell: Cell) -> Option<Cell> {
        if self.last.is_none_or(|last| target > last) {
            return self.insert_inner(target, cell);
        }
        let refresh_tail =
            self.blocks.len() == 1 || self.tail_first.is_none_or(|first| target >= first);
        let result = self.insert_inner(target, cell);
        if refresh_tail && let Some((first, block)) = self.blocks.last_key_value() {
            self.tail_first = Some(*first);
            self.tail_len = block.len();
            self.tail_capacity = block.capacity();
        }
        result
    }
    fn insert_inner(&mut self, target: Key, cell: Cell) -> Option<Cell> {
        debug_assert_eq!(target, key(&cell));
        if self.last.is_none_or(|last| target > last) {
            if let Some(mut last) = self.blocks.last_entry()
                && last.get().len() < BLOCK_CELLS
            {
                self.slots += reserve_growth(last.get_mut());
                last.get_mut().push(cell);
                self.tail_len = last.get().len();
                self.tail_capacity = last.get().capacity();
            } else {
                self.blocks.insert(target, vec![cell]);
                self.slots += 1;
                self.tail_first = Some(target);
                self.tail_len = 1;
                self.tail_capacity = 1;
            }
            self.last = Some(target);
            self.len += 1;
            return None;
        }
        let Some(first) = self
            .blocks
            .range(..=target)
            .next_back()
            .map(|(first, _)| *first)
        else {
            if let Some(first) = self.blocks.first_entry()
                && first.get().len() < BLOCK_CELLS
            {
                let (_, mut block) = first.remove_entry();
                self.slots += reserve_growth(&mut block);
                block.insert(0, cell);
                self.blocks.insert(target, block);
            } else {
                self.blocks.insert(target, vec![cell]);
                self.slots += 1;
            }
            self.len += 1;
            return None;
        };
        let block = self.blocks.get_mut(&first)?;
        let index = match block.binary_search_by_key(&target, key) {
            Ok(index) => return Some(std::mem::replace(&mut block[index], cell)),
            Err(index) => index,
        };
        if block.len() == BLOCK_CELLS {
            if index == block.len() {
                // Monotonic loading fills each block; do not split a complete
                // block into half-empty blocks merely to append its successor.
                self.blocks.insert(target, vec![cell]);
                self.slots += 1;
            } else {
                let capacity = if index < BLOCK_CELLS / 2 {
                    BLOCK_CELLS / 2
                } else {
                    BLOCK_CELLS
                };
                let mut right = Vec::with_capacity(capacity);
                right.extend(block.drain(BLOCK_CELLS / 2..));
                if index < BLOCK_CELLS / 2 {
                    block.insert(index, cell);
                } else {
                    right.insert(index - BLOCK_CELLS / 2, cell);
                }
                let right_key = key(&right[0]);
                self.slots += right.capacity();
                self.blocks.insert(right_key, right);
            }
        } else {
            self.slots += reserve_growth(block);
            block.insert(index, cell);
        }
        self.len += 1;
        None
    }
    pub(crate) fn remove(&mut self, target: &Key, work_available: usize) -> Option<Cell> {
        let first = self
            .blocks
            .range(..=*target)
            .next_back()
            .map(|(first, _)| *first)?;
        let block = self.blocks.get_mut(&first)?;
        let old_capacity = block.capacity();
        let index = block.binary_search_by_key(target, key).ok()?;
        let removed = block.remove(index);
        self.len -= 1;
        if block.is_empty() {
            self.blocks.remove(&first);
            self.slots -= old_capacity;
        } else {
            // Shrink only when the old/new buffer overlap fits the caller's
            // existing operation headroom; removal itself never needs to fail.
            if block.capacity() > block.len().saturating_mul(2)
                && block.len().saturating_mul(size_of::<Cell>()) <= work_available
            {
                block.shrink_to_fit();
                self.slots -= old_capacity - block.capacity();
            }
            if index == 0 {
                let new_first = key(&block[0]);
                if let Some(block) = self.blocks.remove(&first) {
                    self.blocks.insert(new_first, block);
                }
            }
        }
        if self.last == Some(*target) {
            self.last = self
                .blocks
                .last_key_value()
                .and_then(|(_, block)| block.last())
                .map(key);
        }
        if let Some((first, block)) = self.blocks.last_key_value() {
            self.tail_first = Some(*first);
            self.tail_len = block.len();
            self.tail_capacity = block.capacity();
        } else {
            self.tail_first = None;
            self.tail_len = 0;
            self.tail_capacity = 0;
        }
        Some(removed)
    }
    pub(crate) fn retain(&mut self, mut predicate: impl FnMut(&Key, &mut Cell) -> bool) {
        let mut retained = Self::new();
        for (_, mut cell) in std::mem::take(self) {
            let target = key(&cell);
            if predicate(&target, &mut cell) {
                retained.insert(target, cell);
            }
        }
        *self = retained;
    }
    /// Filter without moving coordinates or allocating new blocks. Tree keys
    /// remain lower bounds after removal, so checked cell searches still select
    /// the same ordered block. Later insertions/removals can tighten those bounds.
    pub(crate) fn retain_stationary(&mut self, mut predicate: impl FnMut(&Cell) -> bool) {
        self.blocks.retain(|_, block| {
            block.retain(&mut predicate);
            !block.is_empty()
        });
        self.len = self.blocks.values().map(Vec::len).sum();
        self.slots = self.blocks.values().map(Vec::capacity).sum();
        if let Some((first, block)) = self.blocks.last_key_value() {
            self.last = block.last().map(key);
            self.tail_first = Some(*first);
            self.tail_len = block.len();
            self.tail_capacity = block.capacity();
        } else {
            self.last = None;
            self.tail_first = None;
            self.tail_len = 0;
            self.tail_capacity = 0;
        }
    }
}
fn next_capacity(capacity: usize) -> usize {
    capacity.saturating_mul(2).clamp(4, BLOCK_CELLS)
}
fn growth(len: usize, capacity: usize) -> (usize, usize) {
    if len == 0 || len == BLOCK_CELLS {
        let bytes = size_of::<Cell>() + BLOCK_BYTES;
        (bytes, bytes)
    } else if len == capacity {
        let next = next_capacity(capacity);
        (
            (next - capacity) * size_of::<Cell>(),
            next * size_of::<Cell>(),
        )
    } else {
        (0, 0)
    }
}
fn reserve_growth(block: &mut Vec<Cell>) -> usize {
    let old = block.capacity();
    if block.len() == old {
        block.reserve_exact(next_capacity(old) - block.len());
    }
    block.capacity() - old
}
pub(crate) struct IntoCells {
    blocks: btree_map::IntoValues<Key, Vec<Cell>>,
    current: vec::IntoIter<Cell>,
}
impl Iterator for IntoCells {
    type Item = (Key, Cell);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(cell) = self.current.next() {
                return Some((key(&cell), cell));
            }
            self.current = self.blocks.next()?.into_iter();
        }
    }
}
impl IntoIterator for CellStore {
    type Item = (Key, Cell);
    type IntoIter = IntoCells;
    fn into_iter(self) -> Self::IntoIter {
        IntoCells {
            blocks: self.blocks.into_values(),
            current: Vec::new().into_iter(),
        }
    }
}
