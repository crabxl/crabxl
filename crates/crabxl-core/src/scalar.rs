use crate::{Error, ErrorKind, Result};
use std::sync::Arc;

#[derive(Clone, Debug)]
enum TextStorage {
    Owned(Box<str>),
    Shared(Arc<str>),
}

/// Owned literal text. Indirection keeps common numeric cells compact.
#[derive(Clone, Debug)]
pub struct CellText {
    value: TextStorage,
}
impl CellText {
    /// Own a string without trimming meaningful whitespace.
    pub fn new(value: impl Into<Box<str>>) -> Self {
        Self {
            value: TextStorage::Owned(value.into()),
        }
    }
    /// Borrow the original text.
    pub fn as_str(&self) -> &str {
        match &self.value {
            TextStorage::Owned(value) => value,
            TextStorage::Shared(value) => value,
        }
    }
    /// Retain an immutable shared payload without copying its text. The value
    /// remains independent of the source table/cache lifetime.
    pub fn from_shared(value: Arc<str>) -> Self {
        Self {
            value: TextStorage::Shared(value),
        }
    }
    /// Transfer directly owned text without cloning; shared text is copied into
    /// a detached box because other source/model handles may retain its payload.
    pub fn into_string(self) -> Box<str> {
        match self.value {
            TextStorage::Owned(value) => value,
            TextStorage::Shared(value) => value.as_ref().into(),
        }
    }
    /// Conservative heap charge including the boxed CellValue wrapper. Shared
    /// payloads and reference-count headers are charged per alias, not deduplicated.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.as_str().len()
            + if matches!(self.value, TextStorage::Shared(_)) {
                2 * size_of::<usize>()
            } else {
                0
            }
    }
}
impl PartialEq for CellText {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for CellText {}

/// An exact signed decimal integer exceeding the reader's i64 fast path.
///
/// The canonical decimal representation enables conversion by future adapters
/// without routing the value through floating point.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactInteger {
    decimal: Box<str>,
}
impl ExactInteger {
    /// Validate and canonicalize a signed ASCII decimal integer.
    pub fn parse(value: &str) -> Result<Self> {
        let negative = value.starts_with('-');
        let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid exact integer literal",
            ));
        }
        let significant = digits.trim_start_matches('0');
        let canonical = if significant.is_empty() {
            "0".to_owned()
        } else if negative {
            format!("-{significant}")
        } else {
            significant.to_owned()
        };
        Ok(Self {
            decimal: canonical.into_boxed_str(),
        })
    }
    /// Borrow the canonical decimal representation.
    pub fn as_str(&self) -> &str {
        &self.decimal
    }
    /// Heap allocation including the boxed wrapper when stored in CellValue.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>() + self.decimal.len()
    }
}

/// A literal spreadsheet error, separate from a library I/O/parser error.
/// Unknown future error tokens remain readable without losing their spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellError {
    code: Box<str>,
}
impl CellError {
    /// Own an error token, such as #DIV/0! or #VALUE!.
    pub fn new(code: impl Into<Box<str>>) -> Self {
        Self { code: code.into() }
    }
    /// Borrow the exact error token.
    pub fn as_str(&self) -> &str {
        &self.code
    }
    /// Heap allocation including the boxed wrapper when stored in CellValue.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>() + self.code.len()
    }
}

/// Exact style integer with an allocation-free signed i64 fast path.
/// Large decimal identities are owned and never converted through floating point.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StyleInteger(StyleIntegerRepr);
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum StyleIntegerRepr {
    Small(i64),
    Large(Box<ExactInteger>),
}
impl StyleInteger {
    /// Validate a signed ASCII decimal identity and normalize leading zeros.
    pub fn parse(value: &str) -> Result<Self> {
        if let Ok(value) = value.parse::<i64>() {
            return Ok(value.into());
        }
        let value = ExactInteger::parse(value)?;
        Ok(match value.as_str().parse::<i64>() {
            Ok(value) => value.into(),
            Err(_) => Self(StyleIntegerRepr::Large(Box::new(value))),
        })
    }
    /// Construct an allocation-free small integer.
    pub const fn from_i64(value: i64) -> Self {
        Self(StyleIntegerRepr::Small(value))
    }
    /// Return the small integer when it fits, without coercion.
    pub fn as_i64(&self) -> Option<i64> {
        match self.0 {
            StyleIntegerRepr::Small(value) => Some(value),
            StyleIntegerRepr::Large(_) => None,
        }
    }
    /// Heap payload including the rare boxed exact-integer wrapper.
    pub fn heap_bytes(&self) -> usize {
        match &self.0 {
            StyleIntegerRepr::Small(_) => 0,
            StyleIntegerRepr::Large(value) => value.memory_bytes(),
        }
    }
}
impl From<i64> for StyleInteger {
    fn from(value: i64) -> Self {
        Self::from_i64(value)
    }
}
impl std::fmt::Display for StyleInteger {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            StyleIntegerRepr::Small(value) => write!(output, "{value}"),
            StyleIntegerRepr::Large(value) => output.write_str(value.as_str()),
        }
    }
}
