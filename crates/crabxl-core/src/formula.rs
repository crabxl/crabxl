use crate::{CellValue, Error, ErrorKind, FormulaMetadata, FormulaType, Result};
/// An expression with an optional typed cache and structured encoding metadata;
/// no calculation engine or fabricated cache is implied.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    expression: Box<str>,
    cached: Option<Box<CellValue>>,
    metadata: Option<Box<FormulaMetadata>>,
}
impl Formula {
    /// Construct a formula, stripping one optional leading equals sign.
    pub fn new(expression: impl Into<Box<str>>, cached: Option<CellValue>) -> Result<Self> {
        Self::build(expression.into(), cached, None, true)
    }
    /// Construct a typed shared/array/table record, retaining optional fields.
    pub fn with_metadata(
        expression: impl Into<Box<str>>,
        cached: Option<CellValue>,
        metadata: FormulaMetadata,
    ) -> Result<Self> {
        metadata.validate()?;
        Self::build(expression.into(), cached, Some(Box::new(metadata)), true)
    }
    /// Own an XML expression body verbatim, including an empty body or an
    /// additional equals operator. No literal-call prefix normalization occurs.
    pub fn from_source(
        expression: impl Into<Box<str>>,
        cached: Option<CellValue>,
        metadata: Option<FormulaMetadata>,
    ) -> Result<Self> {
        if let Some(metadata) = &metadata {
            metadata.validate()?;
        }
        Self::build(expression.into(), cached, metadata.map(Box::new), false)
    }
    fn build(
        expression: Box<str>,
        cached: Option<CellValue>,
        metadata: Option<Box<FormulaMetadata>>,
        strip_equals: bool,
    ) -> Result<Self> {
        let expression = if strip_equals {
            expression
                .strip_prefix('=')
                .map(Box::<str>::from)
                .unwrap_or(expression)
        } else {
            expression
        };
        if (strip_equals
            && expression.is_empty()
            && metadata.as_ref().is_none_or(|v| {
                matches!(
                    v.kind,
                    FormulaType::Normal | FormulaType::Shared { master: true, .. }
                )
            }))
            || cached.as_ref().is_some_and(|value| {
                matches!(value, CellValue::Formula(_) | CellValue::RichText(_))
            })
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid normal formula or nested cache",
            ));
        }
        Ok(Self {
            expression,
            cached: cached.map(Box::new),
            metadata,
        })
    }
    /// Stored XML expression body; literal constructors remove one optional equals sign.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    /// Formula encoding category; default expanded expressions are normal.
    pub fn formula_type(&self) -> FormulaType {
        self.metadata
            .as_ref()
            .map_or(FormulaType::Normal, |v| v.kind)
    }
    /// Optional structured source metadata.
    pub fn metadata(&self) -> Option<&FormulaMetadata> {
        self.metadata.as_deref()
    }
    /// Cached result; None means no cached value was supplied.
    pub fn cached(&self) -> Option<&CellValue> {
        self.cached.as_deref()
    }
    /// Owned payload bytes, excluding allocator overhead.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.expression.len()
            + self.metadata.as_ref().map_or(0, |v| v.memory_bytes())
            + self
                .cached
                .as_ref()
                .map_or(0, |value| size_of::<CellValue>() + value.heap_bytes())
    }
}
