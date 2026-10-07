// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Metadata operations.
use super::*;

impl<'a, R: Read + Seek> Rows<'a, R> {
    /// Retain the explicit metadata of the most recently decoded row.
    /// Disabled by default so scalar streaming avoids dimension parsing.
    pub fn capture_dimensions(&mut self) {
        self.capture_dimensions = true;
    }
    /// Borrow metadata for the most recently decoded row, when capture is enabled.
    pub fn row_dimension(&self) -> Option<&crabxl_core::RowDimension> {
        self.row_dimension.as_ref()
    }
    /// Capture compact merge declarations while consuming the tail through EOF.
    /// Scalar streaming leaves this disabled and never expands covered cells.
    pub fn capture_merges(&mut self) {
        self.capture_merges = true;
    }
    /// Borrow captured declarations; complete results require full consumption.
    pub fn merge_ranges(&self) -> &[crabxl_core::CellRange] {
        &self.merge_ranges
    }
    /// Move captured geometry to a caller-owned collection after consumption.
    pub fn take_merge_ranges(&mut self) -> Vec<crabxl_core::CellRange> {
        std::mem::take(&mut self.merge_ranges)
    }
    pub(super) fn merge_bytes(&self) -> usize {
        self.merge_ranges
            .capacity()
            .saturating_mul(size_of::<crabxl_core::CellRange>())
    }
    pub(super) fn retain_merge(&mut self, range: crabxl_core::CellRange) -> Result<()> {
        if self.merge_ranges.len() == self.merge_ranges.capacity() {
            let maximum = (self.limits.max_metadata_bytes.min(usize::MAX as u64) as usize).min(
                self.aggregate.as_ref().map_or(Ok(usize::MAX), |pool| {
                    pool.available(self.shared_formulas.stats().accounted_bytes)
                })?,
            );
            let available = maximum.saturating_sub(self.merge_bytes());
            let growth = self.merge_ranges.capacity().max(4);
            let additional =
                if growth.saturating_mul(size_of::<crabxl_core::CellRange>()) <= available {
                    growth
                } else {
                    1
                };
            if additional.saturating_mul(size_of::<crabxl_core::CellRange>()) > available {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Captured merge geometry exceeds retained allowance",
                )
                .with_part(self.xml.part()));
            }
            let prospective = self
                .merge_bytes()
                .saturating_add(additional.saturating_mul(size_of::<crabxl_core::CellRange>()));
            if let Some(pool) = &self.aggregate {
                let available = pool.available(
                    self.shared_formulas
                        .stats()
                        .accounted_bytes
                        .saturating_add(prospective),
                )?;
                if let Some(strings) = &mut self.shared_strings {
                    if let Some(policy) = self.shared_string_policy {
                        strings.limit_or_spill(policy, available)?
                    } else {
                        strings.limit_managed_bytes(available)?
                    }
                }
            }
            self.merge_ranges
                .try_reserve_exact(additional)
                .map_err(|cause| {
                    Error::caused_by(
                        ErrorKind::MemoryBudgetExceeded,
                        "Cannot reserve captured merge geometry",
                        cause,
                    )
                    .with_part(self.xml.part())
                })?;
            self.limit_string_cache()?;
            if self.merge_bytes() > maximum {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Captured merge capacity exceeds retained allowance",
                )
                .with_part(self.xml.part()));
            }
        }
        self.merge_ranges.push(range);
        Ok(())
    }
}
