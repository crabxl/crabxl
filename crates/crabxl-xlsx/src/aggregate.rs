//! Operation-local shared retained allowance; dependencies and allocator RSS are separate.
use crabxl_core::{Error, ErrorKind, Result};
#[derive(Clone, Copy)]
pub(crate) struct ReadPool {
    pub fixed_bytes: usize,
    pub pool_bytes: usize,
    pub retained_bytes: usize,
}
impl ReadPool {
    pub fn available(&self, component: usize) -> Result<usize> {
        self.pool_bytes
            .checked_sub(self.retained_bytes)
            .and_then(|n| n.checked_sub(component))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Aggregate managed read allowance exceeded",
                )
            })
    }
}
