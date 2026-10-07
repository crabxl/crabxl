// SPDX-License-Identifier: MIT
// URL/tooltip/location separation adapted from umya-spreadsheet,
// Copyright (c) 2020 MathNya. Full optional OOXML fields are CrabXL additions.
//! Canonical hyperlink properties and sparse point ownership.

use crate::{CellAddress, ColumnIndex, Error, ErrorKind, Result, RowIndex};
use std::collections::BTreeMap;

const POINT_BYTES: usize = 256;

/// Resolved hyperlink properties; package relationship IDs are source hints.
/// The destination coordinate belongs to the owning worksheet collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hyperlink {
    /// Relationship target, including meaningful URL fragments and relative URLs.
    pub target: Option<Box<str>>,
    /// Optional destination within the target or current workbook.
    pub location: Option<Box<str>>,
    /// Optional display label, distinct from the cell value.
    pub display: Option<Box<str>>,
    /// Optional tooltip, including an explicit empty spelling.
    pub tooltip: Option<Box<str>>,
    /// Original relationship identity, not a portable workbook-global ID.
    pub relationship_id: Option<Box<str>>,
    /// Whether the relationship target is external to the OPC package.
    pub external: bool,
}
impl Default for Hyperlink {
    fn default() -> Self {
        Self {
            target: None,
            location: None,
            display: None,
            tooltip: None,
            relationship_id: None,
            external: true,
        }
    }
}
impl Hyperlink {
    /// Construct a normal external link without assigning a package identity.
    pub fn external(target: impl Into<Box<str>>) -> Self {
        Self {
            target: Some(target.into()),
            ..Default::default()
        }
    }
    /// Owned text payload; collection node charging includes inline records.
    pub fn heap_bytes(&self) -> usize {
        [
            &self.target,
            &self.location,
            &self.display,
            &self.tooltip,
            &self.relationship_id,
        ]
        .into_iter()
        .map(|v| v.as_ref().map_or(0, |v| v.len()))
        .sum()
    }
}

/// Sparse point links with logarithmic lookup and constant-time byte accounting.
/// Finite range declarations require the later range-aware feature coordinator;
/// callers must not expand them into a dense rectangle.
#[derive(Clone, Debug, Default)]
pub struct Hyperlinks {
    points: BTreeMap<(RowIndex, ColumnIndex), Hyperlink>,
    bytes: usize,
}
impl Hyperlinks {
    /// Number of stored declarations.
    pub fn len(&self) -> usize {
        self.points.len()
    }
    /// Whether the owner has any declarations.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
    /// Managed node and owned-text charge, excluding this inline collection.
    pub fn heap_bytes(&self) -> usize {
        self.bytes
    }
    /// Borrow a declaration without copying its target or tooltip.
    pub fn get(&self, address: CellAddress) -> Option<&Hyperlink> {
        self.points.get(&(address.row, address.column))
    }
    /// Borrow declarations in coordinate order for deterministic output.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (CellAddress, &Hyperlink)> {
        self.points
            .iter()
            .map(|(&(row, column), value)| (CellAddress { row, column }, value))
    }
    /// Remove one declaration and release its managed charge.
    pub fn remove(&mut self, address: CellAddress) -> Option<Hyperlink> {
        let value = self.points.remove(&(address.row, address.column))?;
        self.bytes = self.bytes.saturating_sub(POINT_BYTES + value.heap_bytes());
        Some(value)
    }
    pub(crate) fn proposed_bytes(&self, address: CellAddress, value: Option<&Hyperlink>) -> usize {
        let old = self
            .get(address)
            .map_or(0, |v| POINT_BYTES + v.heap_bytes());
        self.bytes
            .saturating_sub(old)
            .saturating_add(value.map_or(0, |v| POINT_BYTES + v.heap_bytes()))
    }
    /// Replace or remove one declaration within a managed metadata allowance.
    /// Rejection leaves existing declarations and accounting unchanged.
    pub fn set(
        &mut self,
        address: CellAddress,
        value: Option<Hyperlink>,
        maximum: usize,
    ) -> Result<()> {
        let bytes = self.proposed_bytes(address, value.as_ref());
        if bytes > maximum {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Hyperlink metadata allowance exceeded",
            )
            .with_cell(address));
        }
        if let Some(value) = value {
            self.points.insert((address.row, address.column), value);
        } else {
            self.points.remove(&(address.row, address.column));
        }
        self.bytes = bytes;
        Ok(())
    }
    pub(crate) fn non_anchor_intersects(&self, range: crate::CellRange) -> bool {
        use std::ops::Bound::{Excluded, Included};
        self.points
            .range((
                Excluded((range.start.row, range.start.column)),
                Included((range.end.row, range.end.column)),
            ))
            .any(|(&(row, column), _)| range.contains(CellAddress { row, column }))
    }
    pub(crate) fn intersects(&self, range: crate::CellRange) -> bool {
        self.points
            .range((range.start.row, range.start.column)..=(range.end.row, range.end.column))
            .any(|(&(row, column), _)| range.contains(CellAddress { row, column }))
    }
}
