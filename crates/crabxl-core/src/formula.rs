use crate::{CellValue, Error, ErrorKind, Result};
/// A normal formula with an optional typed cached result; no calculation engine.
/// Shared/array/data-table formula metadata is tracked separately for M2/M5.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    expression: Box<str>,
    cached: Option<Box<CellValue>>,
}
impl Formula {
    /// Construct a formula, stripping one optional leading equals sign.
    pub fn new(expression: impl Into<Box<str>>, cached: Option<CellValue>) -> Result<Self> {
        let expression = expression.into();
        let expression = expression
            .strip_prefix('=')
            .map(Box::<str>::from)
            .unwrap_or(expression);
        if expression.is_empty()
            || cached
                .as_ref()
                .is_some_and(|value| matches!(value, CellValue::Formula(_)))
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid normal formula or nested cache",
            ));
        }
        Ok(Self {
            expression,
            cached: cached.map(Box::new),
        })
    }
    /// Expression without its leading equals sign.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    /// Cached result; None means no cached value was supplied.
    pub fn cached(&self) -> Option<&CellValue> {
        self.cached.as_deref()
    }
    /// Owned payload bytes, excluding allocator overhead.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.expression.len()
            + self
                .cached
                .as_ref()
                .map_or(0, |value| size_of::<CellValue>() + value.heap_bytes())
    }
}
