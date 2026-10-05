//! Ordered sparse cells in bounded contiguous blocks, without per-cell tree nodes.
use crate::Cell;
use std::{
    collections::{BTreeMap, btree_map},
    ops::RangeInclusive,
    vec,
};

type Key = (u32, u32);
const BLOCK_CELLS: usize = 128;

#[derive(Default)]
pub(crate) struct CellStore {
    blocks: BTreeMap<Key, Vec<Cell>>,
    len: usize,
    last: Option<Key>,
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
        debug_assert_eq!(target, key(&cell));
        if self.last.is_none_or(|last| target > last) {
            if let Some(mut last) = self.blocks.last_entry()
                && last.get().len() < BLOCK_CELLS
            {
                last.get_mut().push(cell);
            } else {
                self.blocks.insert(target, vec![cell]);
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
            self.blocks.insert(target, vec![cell]);
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
            } else {
                let mut right = block.split_off(BLOCK_CELLS / 2);
                if index < BLOCK_CELLS / 2 {
                    block.insert(index, cell);
                } else {
                    right.insert(index - BLOCK_CELLS / 2, cell);
                }
                let right_key = key(&right[0]);
                self.blocks.insert(right_key, right);
            }
        } else {
            block.insert(index, cell);
        }
        self.len += 1;
        None
    }
    pub(crate) fn remove(&mut self, target: &Key) -> Option<Cell> {
        let first = self
            .blocks
            .range(..=*target)
            .next_back()
            .map(|(first, _)| *first)?;
        let block = self.blocks.get_mut(&first)?;
        let index = block.binary_search_by_key(target, key).ok()?;
        let removed = block.remove(index);
        self.len -= 1;
        if block.is_empty() {
            self.blocks.remove(&first);
        } else {
            if block.capacity() > block.len().saturating_mul(2) {
                block.shrink_to_fit();
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
