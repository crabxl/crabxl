// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Budget operations.
use super::*;

impl<'a, R: Read + Seek> Rows<'a, R> {
    pub(crate) fn with_read_pool(
        mut self,
        pool: Option<crate::aggregate::ReadPool>,
        string_policy: &'a crate::SharedStringOptions,
    ) -> Result<Self> {
        self.aggregate = pool;
        self.shared_string_policy = pool.map(|_| string_policy);
        self.limit_string_cache()?;
        Ok(self)
    }
    pub(crate) fn policy_catalog_bytes(&self) -> usize {
        self.aggregate
            .as_ref()
            .map_or(0, |pool| pool.fixed_bytes)
            .saturating_add(
                self.shared_strings
                    .as_ref()
                    .map_or(0, |strings| strings.minimum_managed_bytes()),
            )
    }
    pub(crate) fn shared_cache_bytes(&self) -> usize {
        self.shared_strings.as_ref().map_or(0, |strings| {
            strings
                .stats()
                .managed_bytes
                .saturating_sub(strings.minimum_managed_bytes())
        })
    }
    pub(crate) fn available_retained_bytes(&self) -> Result<usize> {
        self.aggregate.as_ref().map_or(Ok(usize::MAX), |pool| {
            let shared = self
                .shared_strings
                .as_ref()
                .map_or(0, |strings| strings.minimum_managed_bytes());
            pool.pool_bytes
                .checked_sub(shared)
                .and_then(|n| n.checked_sub(self.shared_formulas.stats().accounted_bytes))
                .and_then(|n| n.checked_sub(self.captured_metadata_bytes()))
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Aggregate component allowance exceeded",
                    )
                })
        })
    }
    pub(crate) fn set_aggregate_retained(&mut self, bytes: usize) -> Result<()> {
        if let Some(pool) = &mut self.aggregate {
            pool.retained_bytes = bytes;
        }
        self.limit_string_cache()
    }
    pub(super) fn limit_string_cache(&mut self) -> Result<()> {
        if let Some(pool) = &self.aggregate {
            let maximum = pool.available(
                self.shared_formulas
                    .stats()
                    .accounted_bytes
                    .saturating_add(self.captured_metadata_bytes()),
            )?;
            if let Some(strings) = &mut self.shared_strings {
                if let Some(policy) = self.shared_string_policy {
                    strings.limit_or_spill(policy, maximum)?;
                } else {
                    strings.limit_managed_bytes(maximum)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn limit_formula_storage(
        &mut self,
        metadata: &crabxl_core::FormulaMetadata,
        expression: &str,
    ) -> Result<()> {
        if let (Some(pool), crabxl_core::FormulaType::Shared { index, .. }) =
            (&self.aggregate, &metadata.kind)
        {
            let required = self.shared_formulas.required_bytes(index, expression.len());
            let available = pool.available(required)?;
            if let Some(strings) = &mut self.shared_strings {
                if let Some(policy) = self.shared_string_policy {
                    strings.limit_or_spill(policy, available)?;
                } else {
                    strings.limit_managed_bytes(available)?;
                }
            }
            let strings = self
                .shared_strings
                .as_ref()
                .map_or(0, |strings| strings.stats().managed_bytes);
            self.shared_formulas.set_maximum(
                pool.available(strings)?
                    .min(self.limits.max_formula_table_bytes),
            );
        }
        Ok(())
    }
    /// Live managed retained storage participating in this policy operation.
    /// Working reserve and caller-retained outputs are reported separately.
    pub fn managed_retained_bytes(&self) -> usize {
        let shared = self
            .shared_strings
            .as_ref()
            .map_or(0, |strings| strings.stats().managed_bytes);
        let formula = self
            .shared_formulas
            .stats()
            .accounted_bytes
            .saturating_add(self.captured_metadata_bytes());
        self.aggregate
            .as_ref()
            .map_or(shared.saturating_add(formula), |pool| {
                pool.fixed_bytes
                    .saturating_add(pool.retained_bytes)
                    .saturating_add(shared)
                    .saturating_add(formula)
            })
    }
}
