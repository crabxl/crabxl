//! Compact coverage edits; one replacement creates at most four pieces per range.
use super::*;
use crate::CellRange;

pub(super) const RANGE_BYTES: usize = 256 + size_of::<CellRange>();

impl Hyperlinks {
    fn edit_area(
        address: CellAddress,
        value: Option<&Hyperlink>,
        coverage: bool,
    ) -> Result<CellRange> {
        if let Some(link) = value {
            link.validate_owner(address)?;
            if coverage && let Some(reference) = &link.reference {
                let range: CellRange = reference.parse()?;
                if range.start != range.end {
                    if range.start != address {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "Range declaration must start at its owner",
                        )
                        .with_cell(address));
                    }
                    return Ok(range);
                }
            }
        }
        CellRange::new(address, address)
    }
    pub(super) fn needs_coverage_edit(
        &self,
        address: CellAddress,
        _value: Option<&Hyperlink>,
    ) -> bool {
        self.ranges.values().any(|range| range.contains(address))
    }
    fn overlaps(&self, owner: (RowIndex, ColumnIndex), area: CellRange) -> bool {
        self.ranges.get(&owner).map_or_else(
            || {
                area.contains(CellAddress {
                    row: owner.0,
                    column: owner.1,
                })
            },
            |range| range.intersects(area),
        )
    }
    fn record_bytes(coverage: bool, link: &Hyperlink) -> usize {
        POINT_BYTES + link.heap_bytes() + usize::from(coverage) * RANGE_BYTES
    }
    fn fragment_bytes(link: &Hyperlink, range: CellRange) -> usize {
        let reference = if range.start == range.end {
            0
        } else {
            range.to_string().len()
        };
        POINT_BYTES
            + link
                .heap_bytes()
                .saturating_sub(link.reference.as_ref().map_or(0, |value| value.len()))
            + reference
            + usize::from(range.start != range.end) * RANGE_BYTES
    }
    pub(super) fn coverage_plan(
        &self,
        address: CellAddress,
        value: Option<&Hyperlink>,
        coverage: bool,
    ) -> Result<(usize, usize)> {
        let area = Self::edit_area(address, value, coverage)?;
        let mut bytes = self.bytes;
        let mut peak = bytes;
        for (&owner, link) in &self.points {
            if !self.overlaps(owner, area) {
                continue;
            }
            bytes =
                bytes.saturating_sub(Self::record_bytes(self.ranges.contains_key(&owner), link));
            if let Some(range) = self.ranges.get(&owner) {
                let pieces = range.difference(area);
                bytes = bytes.saturating_add(
                    pieces
                        .into_iter()
                        .flatten()
                        .map(|piece| Self::fragment_bytes(link, piece))
                        .sum::<usize>(),
                );
                // The removed owned payload stays alive while its pieces are made.
                peak = peak.max(bytes.saturating_add(link.heap_bytes()));
            }
        }
        bytes = bytes.saturating_add(value.map_or(0, |link| {
            Self::record_bytes(coverage && area.start != area.end, link)
        }));
        Ok((bytes, peak.max(bytes)))
    }
    /// Managed peak during a compact coverage replacement, including split scratch.
    pub fn replacement_peak_bytes(&self, address: CellAddress, value: Option<&Hyperlink>) -> usize {
        if self.needs_coverage_edit(address, value) {
            self.coverage_plan(address, value, false)
                .map_or(usize::MAX, |(_, peak)| peak)
        } else {
            self.replacement_bytes(address, value)
        }
    }
    fn insert_record(
        &mut self,
        address: CellAddress,
        link: Hyperlink,
        coverage: Option<CellRange>,
    ) {
        let owner = (address.row, address.column);
        if let Some(range) = coverage {
            self.ranges.insert(owner, range);
        }
        self.bytes = self
            .bytes
            .saturating_add(Self::record_bytes(coverage.is_some(), &link));
        self.references += usize::from(link.reference.is_some());
        self.points.insert(owner, link);
    }
    pub(super) fn set_coverage(
        &mut self,
        address: CellAddress,
        value: Option<Hyperlink>,
        maximum: usize,
    ) -> Result<()> {
        self.replace_coverage(address, value, false, maximum)
    }
    /// Adopt a declaration's actual range without expanding covered coordinates.
    /// Point assignments keep their independent serialized reference semantics.
    pub fn set_declaration(
        &mut self,
        address: CellAddress,
        link: Hyperlink,
        maximum: usize,
    ) -> Result<()> {
        let area = Self::edit_area(address, Some(&link), true)?;
        if area.start == area.end {
            self.set(address, Some(link), maximum)
        } else {
            self.replace_coverage(address, Some(link), true, maximum)
        }
    }
    /// Managed peak for adopting a decoded range declaration.
    pub fn declaration_peak_bytes(&self, address: CellAddress, link: &Hyperlink) -> Result<usize> {
        let area = Self::edit_area(address, Some(link), true)?;
        if area.start == area.end {
            Ok(self.replacement_peak_bytes(address, Some(link)))
        } else {
            self.coverage_plan(address, Some(link), true)
                .map(|(_, peak)| peak)
        }
    }
    fn replace_coverage(
        &mut self,
        address: CellAddress,
        value: Option<Hyperlink>,
        coverage: bool,
        maximum: usize,
    ) -> Result<()> {
        let area = Self::edit_area(address, value.as_ref(), coverage)?;
        let (_, peak) = self.coverage_plan(address, value.as_ref(), coverage)?;
        if peak > maximum {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Hyperlink range replacement exceeds metadata allowance",
            )
            .with_cell(address));
        }
        while let Some(owner) = self
            .ranges
            .iter()
            .find(|(_, range)| range.intersects(area))
            .map(|(owner, _)| *owner)
        {
            let range = self.ranges.get(&owner).copied();
            let old_address = CellAddress {
                row: owner.0,
                column: owner.1,
            };
            let Some(old) = self.remove(old_address) else {
                return Err(Error::new(
                    ErrorKind::InvalidState,
                    "Missing hyperlink coverage record",
                ));
            };
            if let Some(range) = range {
                for piece in range.difference(area).into_iter().flatten() {
                    let mut link = old.clone();
                    link.reference =
                        (piece.start != piece.end).then(|| piece.to_string().into_boxed_str());
                    self.insert_record(
                        piece.start,
                        link,
                        (piece.start != piece.end).then_some(piece),
                    );
                }
            }
        }
        // Walk point declarations once; do not restart at retained coordinates
        // after every deletion from a large rectangle.
        use std::ops::Bound::{Excluded, Included};
        let mut lower = Included((area.start.row, area.start.column));
        let upper = Included((area.end.row, area.end.column));
        loop {
            let next = self
                .points
                .range((lower, upper))
                .find(|(owner, _)| {
                    area.contains(CellAddress {
                        row: owner.0,
                        column: owner.1,
                    })
                })
                .map(|(owner, _)| *owner);
            let Some(owner) = next else {
                break;
            };
            self.remove(CellAddress {
                row: owner.0,
                column: owner.1,
            });
            lower = Excluded(owner);
        }
        if let Some(link) = value {
            self.insert_record(
                address,
                link,
                (coverage && area.start != area.end).then_some(area),
            );
        }
        Ok(())
    }
    /// Clear one covered coordinate without removing the rest of its declaration.
    pub fn clear(&mut self, address: CellAddress, maximum: usize) -> Result<Option<Hyperlink>> {
        if !self.ranges.values().any(|range| range.contains(address)) {
            return Ok(self.remove(address));
        }
        let returned_bytes = self.get(address).map_or(0, Hyperlink::heap_bytes);
        let available = maximum.saturating_sub(returned_bytes);
        let (_, peak) = self.coverage_plan(address, None, false)?;
        if peak > available {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Hyperlink clearing exceeds metadata allowance",
            )
            .with_cell(address));
        }
        let value = self.get(address).cloned();
        self.set(address, None, available)?;
        Ok(value)
    }
    /// Actual decoded range coverage, separate from independently mutable refs.
    pub fn covering_range(&self, address: CellAddress) -> Option<CellRange> {
        self.ranges
            .values()
            .copied()
            .find(|range| range.contains(address))
    }
}
