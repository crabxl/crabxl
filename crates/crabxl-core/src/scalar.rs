use crate::{Error, ErrorKind, Result};

/// Owned literal text. Indirection keeps common numeric cells compact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellText {
    value: Box<str>,
}
impl CellText {
    /// Own a string without trimming meaningful whitespace.
    pub fn new(value: impl Into<Box<str>>) -> Self {
        Self {
            value: value.into(),
        }
    }
    /// Borrow the original text.
    pub fn as_str(&self) -> &str {
        &self.value
    }
    /// Transfer the owned text without cloning its payload.
    pub fn into_string(self) -> Box<str> {
        self.value
    }
    /// Heap allocation including the boxed wrapper when stored in CellValue.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>() + self.value.len()
    }
}

/// An exact signed decimal integer exceeding the reader's i64 fast path.
///
/// The canonical decimal representation enables conversion by future adapters
/// without routing the value through floating point.
#[derive(Clone, Debug, PartialEq, Eq)]
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
