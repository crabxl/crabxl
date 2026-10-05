//! Worksheet-local literal shared-formula identities.
use std::sync::Arc;
/// Source shared-formula group identity. Missing and empty identifiers are
/// distinct; noncanonical spelling is not normalized into another group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharedFormulaIndex {
    /// No source si attribute; compatible groups may use this identity.
    Missing,
    /// Canonical unsigned decimal spelling, without a retained string allocation.
    Numeric(u32),
    /// Literal source spelling shared by the template and emitted metadata.
    Literal(Arc<str>),
}
impl SharedFormulaIndex {
    /// Own a public source property, optimizing only canonical u32 decimals.
    pub fn from_literal(value: &str) -> Self {
        if (value == "0"
            || value
                .as_bytes()
                .first()
                .is_some_and(|byte| matches!(byte, b'1'..=b'9')))
            && let Ok(number) = value.parse::<u32>()
        {
            return Self::Numeric(number);
        }
        Self::Literal(Arc::from(value))
    }
    /// Literal spelling when one is retained; missing/numeric variants allocate none.
    pub fn literal(&self) -> Option<&str> {
        match self {
            Self::Literal(value) => Some(value),
            _ => None,
        }
    }
    /// Source string bytes, excluding the shared ownership header.
    pub fn payload_bytes(&self) -> usize {
        self.literal().map_or(0, str::len)
    }
    /// Shared string body/header bytes. Per-value estimates are conservative
    /// when another value owns the same immutable allocation.
    pub fn heap_bytes(&self) -> usize {
        match self {
            Self::Literal(value) => value.len().saturating_add(2 * size_of::<usize>()),
            _ => 0,
        }
    }
}
impl From<u32> for SharedFormulaIndex {
    fn from(value: u32) -> Self {
        Self::Numeric(value)
    }
}
impl std::fmt::Display for SharedFormulaIndex {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => Ok(()),
            Self::Numeric(value) => value.fmt(output),
            Self::Literal(value) => output.write_str(value),
        }
    }
}
impl std::hash::Hash for SharedFormulaIndex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Numeric(value) => value.hash(state),
            Self::Literal(value) => value.hash(state),
            Self::Missing => u64::MAX.hash(state),
        }
    }
}
