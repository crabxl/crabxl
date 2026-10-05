use crate::{CellValue, Error, ErrorKind, FormulaMetadata, FormulaType, Result};
use std::borrow::Cow;
/// An expression with an optional typed cache and structured encoding metadata;
/// no calculation engine or fabricated cache is implied.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    expression: Option<Box<str>>,
    cached: Option<Box<CellValue>>,
    metadata: Option<Box<FormulaMetadata>>,
}
impl Formula {
    /// Construct a formula, stripping one optional leading equals sign.
    pub fn new(expression: impl Into<Box<str>>, cached: Option<CellValue>) -> Result<Self> {
        Self::build(Some(expression.into()), cached, None, true)
    }
    /// Construct a typed shared/array/table record, retaining optional fields.
    pub fn with_metadata(
        expression: impl Into<Box<str>>,
        cached: Option<CellValue>,
        metadata: FormulaMetadata,
    ) -> Result<Self> {
        metadata.validate()?;
        Self::build(
            Some(expression.into()),
            cached,
            Some(Box::new(metadata)),
            true,
        )
    }
    /// Construct an array/data-table value whose expression may be absent.
    /// Absence is distinct from an explicit empty literal before serialization.
    /// Reading an empty XML body creates a present empty source expression.
    pub fn with_optional_expression(
        expression: Option<Box<str>>,
        cached: Option<CellValue>,
        metadata: FormulaMetadata,
    ) -> Result<Self> {
        if !matches!(metadata.kind, FormulaType::Array | FormulaType::DataTable) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Optional expression requires an array or data-table formula",
            ));
        }
        metadata.validate()?;
        Self::build(expression, cached, Some(Box::new(metadata)), true)
    }
    /// Retain a public literal array text property in a single owned payload.
    /// XML serialization removes the first Unicode character, even when it is
    /// not an equals sign, matching the pinned reference's array-object behavior.
    pub fn from_array_text(
        text: Option<Box<str>>,
        cached: Option<CellValue>,
        mut metadata: FormulaMetadata,
    ) -> Result<Self> {
        metadata.literal_array_text = true;
        metadata.validate()?;
        Self::build(text, cached, Some(Box::new(metadata)), false)
    }
    /// Own an XML expression body verbatim, including an empty body or an
    /// additional equals operator. No literal-call prefix normalization occurs.
    pub fn from_source(
        expression: impl Into<Box<str>>,
        cached: Option<CellValue>,
        mut metadata: Option<FormulaMetadata>,
    ) -> Result<Self> {
        if let Some(metadata) = &mut metadata {
            metadata.literal_array_text = false;
            metadata.validate()?;
        }
        Self::build(
            Some(expression.into()),
            cached,
            metadata.map(Box::new),
            false,
        )
    }
    fn build(
        expression: Option<Box<str>>,
        cached: Option<CellValue>,
        metadata: Option<Box<FormulaMetadata>>,
        strip_equals: bool,
    ) -> Result<Self> {
        let strip_equals = strip_equals
            && metadata
                .as_ref()
                .is_none_or(|metadata| !metadata.literal_array_text);
        let expression = if strip_equals {
            expression.map(|expression| {
                expression
                    .strip_prefix('=')
                    .map(Box::<str>::from)
                    .unwrap_or(expression)
            })
        } else {
            expression
        };
        if (strip_equals
            && expression.as_deref().is_none_or(str::is_empty)
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
        let expression = self.expression.as_deref().unwrap_or_default();
        if self
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.literal_array_text)
        {
            expression
                .chars()
                .next()
                .map_or("", |first| &expression[first.len_utf8()..])
        } else {
            expression
        }
    }
    /// Literal/source expression presence, independent of the XML body's empty view.
    pub fn optional_expression(&self) -> Option<&str> {
        self.expression.as_ref().map(|_| self.expression())
    }
    /// Public array text spelling, borrowing owned literals and formatting source
    /// XML bodies only when requested. None retains literal property absence.
    pub fn array_text(&self) -> Option<Cow<'_, str>> {
        if self.formula_type() != FormulaType::Array {
            return None;
        }
        self.expression.as_deref().map(|text| {
            if self
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.literal_array_text)
            {
                Cow::Borrowed(text)
            } else {
                Cow::Owned(format!("={text}"))
            }
        })
    }
    /// Formula encoding category; default expanded expressions are normal.
    pub fn formula_type(&self) -> FormulaType {
        self.metadata
            .as_ref()
            .map_or(FormulaType::Normal, |v| v.kind.clone())
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
            + self
                .expression
                .as_ref()
                .map_or(0, |expression| expression.len())
            + self.metadata.as_ref().map_or(0, |v| v.memory_bytes())
            + self
                .cached
                .as_ref()
                .map_or(0, |value| size_of::<CellValue>() + value.heap_bytes())
    }
}
