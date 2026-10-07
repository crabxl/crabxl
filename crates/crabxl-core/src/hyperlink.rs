// SPDX-License-Identifier: MIT
// URL/tooltip/location separation adapted from umya-spreadsheet,
// Copyright (c) 2020 MathNya. Full optional OOXML fields are CrabXL additions.
//! Canonical hyperlink properties and sparse point ownership.

use crate::{CellAddress, ColumnIndex, Error, ErrorKind, Result, RowIndex};
use std::collections::BTreeMap;
mod ranges;

const POINT_BYTES: usize = 256;

/// Resolved hyperlink properties; package relationship IDs are source hints.
/// The owner coordinate belongs to the worksheet; an optional declaration reference
/// preserves independently mutable public metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hyperlink {
    /// Optional serialized declaration reference, independent of the owner cell.
    pub reference: Option<Box<str>>,
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
            reference: None,
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
    /// Validate a finite declaration reference before committing metadata.
    pub fn validate_reference(&self) -> Result<()> {
        if let Some(reference) = &self.reference {
            let _: crate::CellRange = reference.parse()?;
        }
        Ok(())
    }
    /// Validate declaration spelling with the owning cell's error context.
    pub fn validate_owner(&self, address: CellAddress) -> Result<()> {
        self.validate_reference()
            .map_err(|error| error.with_cell(address))
    }
    /// Initial value for an empty anchor, with compatible empty-target fallback.
    pub fn initial_cell_value(&self) -> crate::CellValue {
        self.target
            .as_ref()
            .filter(|text| !text.is_empty())
            .or(self.location.as_ref())
            .map_or(crate::CellValue::Empty, |text| {
                crate::CellValue::text(text.clone())
            })
    }
    /// Owned text payload; collection node charging includes inline records.
    pub fn heap_bytes(&self) -> usize {
        [
            &self.reference,
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
/// Range metadata remains compact; covered coordinates borrow the same payload.
#[derive(Clone, Debug, Default)]
pub struct Hyperlinks {
    points: BTreeMap<(RowIndex, ColumnIndex), Hyperlink>,
    bytes: usize,
    references: usize,
    ranges: BTreeMap<(RowIndex, ColumnIndex), crate::CellRange>,
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
        self.points.get(&(address.row, address.column)).or_else(|| {
            self.ranges
                .iter()
                .find(|(_, range)| range.contains(address))
                .and_then(|(owner, _)| self.points.get(owner))
        })
    }
    /// Borrow declarations in coordinate order for deterministic output.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (CellAddress, &Hyperlink)> {
        self.points
            .iter()
            .map(|(&(row, column), value)| (CellAddress { row, column }, value))
    }
    /// Resolve one retained relationship target without cloning unrelated fields.
    /// A failed budget check leaves this declaration unchanged.
    pub fn set_relationship_target(
        &mut self,
        address: CellAddress,
        target: Box<str>,
        external: bool,
        maximum: usize,
    ) -> Result<()> {
        let old = self.get(address).ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "Unknown hyperlink declaration").with_cell(address)
        })?;
        let bytes = self
            .bytes
            .saturating_sub(old.target.as_ref().map_or(0, |value| value.len()))
            .saturating_add(target.len());
        if bytes > maximum {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Resolved hyperlink target exceeds allowance",
            )
            .with_cell(address));
        }
        if let Some(link) = self.points.get_mut(&(address.row, address.column)) {
            link.target = Some(target);
            link.external = external;
        }
        self.bytes = bytes;
        Ok(())
    }
    /// Remove one declaration and release its managed charge.
    pub fn remove(&mut self, address: CellAddress) -> Option<Hyperlink> {
        let value = self.points.remove(&(address.row, address.column))?;
        let range = self.ranges.remove(&(address.row, address.column));
        self.bytes = self.bytes.saturating_sub(
            POINT_BYTES + value.heap_bytes() + usize::from(range.is_some()) * ranges::RANGE_BYTES,
        );
        self.references -= usize::from(value.reference.is_some());
        Some(value)
    }
    /// Managed charge after replacing one declaration, without mutation.
    pub fn replacement_bytes(&self, address: CellAddress, value: Option<&Hyperlink>) -> usize {
        if self.needs_coverage_edit(address, value) {
            return self
                .coverage_plan(address, value, false)
                .map_or(usize::MAX, |(bytes, _)| bytes);
        }
        let old = self
            .points
            .get(&(address.row, address.column))
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
        if let Some(link) = &value {
            link.validate_owner(address)
                .map_err(|error| error.with_cell(address))?;
        }
        if self.needs_coverage_edit(address, value.as_ref()) {
            return self.set_coverage(address, value, maximum);
        }
        let bytes = self.replacement_bytes(address, value.as_ref());
        if bytes > maximum {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Hyperlink metadata allowance exceeded",
            )
            .with_cell(address));
        }
        let old_reference = usize::from(
            self.get(address)
                .is_some_and(|link| link.reference.is_some()),
        );
        let new_reference =
            usize::from(value.as_ref().is_some_and(|link| link.reference.is_some()));
        self.references = self.references - old_reference + new_reference;
        if let Some(value) = value {
            self.points.insert((address.row, address.column), value);
        } else {
            self.points.remove(&(address.row, address.column));
        }
        self.bytes = bytes;
        Ok(())
    }
    fn reference_intersects(&self, range: crate::CellRange, non_anchor: bool) -> bool {
        if self.references == 0 {
            return false;
        }
        self.points.values().any(|link| {
            link.reference.as_deref().is_some_and(|reference| {
                reference
                    .parse::<crate::CellRange>()
                    .map_or(true, |declared| {
                        range.intersects(declared)
                            && (!non_anchor
                                || declared.start != range.start
                                || declared.end != range.start)
                    })
            })
        })
    }
    pub(crate) fn non_anchor_intersects(&self, range: crate::CellRange) -> bool {
        use std::ops::Bound::{Excluded, Included};
        self.reference_intersects(range, true)
            || self
                .points
                .range((
                    Excluded((range.start.row, range.start.column)),
                    Included((range.end.row, range.end.column)),
                ))
                .any(|(&(row, column), _)| range.contains(CellAddress { row, column }))
    }
    pub(crate) fn intersects(&self, range: crate::CellRange) -> bool {
        self.reference_intersects(range, false)
            || self
                .points
                .range((range.start.row, range.start.column)..=(range.end.row, range.end.column))
                .any(|(&(row, column), _)| range.contains(CellAddress { row, column }))
    }
}
