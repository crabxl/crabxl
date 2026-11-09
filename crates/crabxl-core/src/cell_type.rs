//! Explicit OOXML storage categories without changing the caller-visible value.
use crate::{CellValue, Error, ErrorKind, ExactInteger, Formula, FormulaType, Result};
use std::borrow::Cow;

/// A caller-selected cell storage category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellStorageType {
    /// Numeric literal.
    Number,
    /// Inline text or rich text.
    String,
    /// Boolean literal.
    Boolean,
    /// Date/time literal.
    Date,
    /// Formula expression.
    Formula,
    /// Spreadsheet error literal.
    Error,
    /// A literal stored in the formula-string representation.
    FormulaString,
    /// An explicitly empty inline-string record.
    InlineString,
}
impl CellStorageType {
    /// Validate a public storage token.
    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "n" => Self::Number,
            "s" => Self::String,
            "b" => Self::Boolean,
            "d" => Self::Date,
            "f" => Self::Formula,
            "e" => Self::Error,
            "str" => Self::FormulaString,
            "inlineStr" => Self::InlineString,
            _ => return Err(invalid()),
        })
    }
    /// Public token spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Number => "n",
            Self::String => "s",
            Self::Boolean => "b",
            Self::Date => "d",
            Self::Formula => "f",
            Self::Error => "e",
            Self::FormulaString => "str",
            Self::InlineString => "inlineStr",
        }
    }
}
fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "Invalid cell storage type or value")
}
impl CellValue {
    /// Explicit storage category, independent of the raw scalar's classification.
    pub fn storage_type(&self) -> Option<CellStorageType> {
        match self {
            Self::Annotated(value) => value.storage_type.or_else(|| value.value.storage_type()),
            _ => None,
        }
    }
    /// Change the storage category while retaining value metadata and the logical value.
    pub fn with_storage_type(mut self, kind: CellStorageType) -> Result<Self> {
        if self.storage_type() == Some(kind) {
            return Ok(self);
        }
        let automatic = match self.unannotated() {
            Self::Empty | Self::Number(_) | Self::Integer(_) | Self::BigInteger(_) => {
                CellStorageType::Number
            }
            Self::Text(_) | Self::RichText(_) => CellStorageType::String,
            Self::Boolean(_) => CellStorageType::Boolean,
            Self::DateTime(_) => CellStorageType::Date,
            Self::Formula(_) => CellStorageType::Formula,
            Self::Error(_) => CellStorageType::Error,
            _ => return Err(invalid()),
        };
        if self.storage_type().is_none() && automatic == kind {
            return Ok(self);
        }
        let annotations = self.annotations().cloned().unwrap_or_default();
        if matches!(
            kind,
            CellStorageType::String | CellStorageType::FormulaString | CellStorageType::Error
        ) && let Self::Formula(formula) = self.unannotated()
            && matches!(
                formula.formula_type(),
                FormulaType::Normal | FormulaType::Shared { .. }
            )
        {
            let text = format!("={}", formula.expression());
            *self.unannotated_mut() = Self::text(text);
        } else if kind == CellStorageType::Formula
            && let Self::Error(text) = self.unannotated()
        {
            *self.unannotated_mut() = Self::text(text.as_str());
        } else if kind == CellStorageType::Formula
            && let Self::Text(text) = self.unannotated()
            && let Some(expression) = text.as_str().strip_prefix('=')
        {
            let formula = Formula::from_source(expression, None, None)?;
            *self.unannotated_mut() = Self::Formula(Box::new(formula));
        }
        match self {
            Self::Annotated(mut value) => {
                value.storage_type = Some(kind);
                Ok(Self::Annotated(value))
            }
            value => Ok(Self::Annotated(Box::new(crate::AnnotatedValue {
                value,
                annotations,
                storage_type: Some(kind),
            }))),
        }
    }
    /// Resolve the write payload without changing the stored, caller-visible value.
    pub fn storage_value(&self) -> Result<Cow<'_, CellValue>> {
        let value = self.unannotated();
        let Some(kind) = self.storage_type() else {
            return Ok(Cow::Borrowed(value));
        };
        if matches!(value, Self::Empty) {
            return Ok(Cow::Borrowed(value));
        }
        let owned = match (kind, value) {
            (CellStorageType::Number, Self::Number(_) | Self::Integer(_) | Self::BigInteger(_))
            | (CellStorageType::String, Self::Text(_) | Self::RichText(_))
            | (CellStorageType::FormulaString, Self::Text(_))
            | (CellStorageType::Boolean, Self::Boolean(_))
            | (CellStorageType::Date, Self::DateTime(_))
            | (CellStorageType::Formula, Self::Formula(_))
            | (CellStorageType::Error, Self::Error(_)) => return Ok(Cow::Borrowed(value)),
            (CellStorageType::Number, Self::Boolean(value)) => Self::Integer(i64::from(*value)),
            (CellStorageType::Number, Self::Text(value)) => {
                let text = value.as_str();
                if let Ok(number) = text.parse::<i64>() {
                    Self::Integer(number)
                } else if let Ok(number) = ExactInteger::parse(text) {
                    Self::BigInteger(Box::new(number))
                } else {
                    let number = text.parse::<f64>().map_err(|_| invalid())?;
                    if !number.is_finite() {
                        return Err(invalid());
                    }
                    Self::Number(number)
                }
            }
            (CellStorageType::String | CellStorageType::FormulaString, Self::Error(value)) => {
                Self::text(value.as_str())
            }
            (CellStorageType::FormulaString, Self::RichText(value)) => {
                Self::text(value.plain_text()?)
            }
            (CellStorageType::FormulaString, Self::Integer(value)) => Self::text(value.to_string()),
            (CellStorageType::FormulaString, Self::BigInteger(value)) => Self::text(value.as_str()),
            (CellStorageType::FormulaString, Self::Boolean(value)) => {
                Self::text(if *value { "1" } else { "0" })
            }
            (CellStorageType::Boolean, Self::Integer(value)) => Self::Boolean(*value != 0),
            (CellStorageType::Boolean, Self::Number(value)) if *value == 0.0 || *value == 1.0 => {
                Self::Boolean(*value == 1.0)
            }
            (CellStorageType::Boolean, Self::Text(value)) => Self::Boolean(
                value
                    .as_str()
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| invalid())?
                    != 0,
            ),
            (CellStorageType::Error, Self::Text(value)) => Self::error(value.as_str()),
            (CellStorageType::Formula, Self::Text(value)) => {
                let text = value.as_str();
                let offset = text.chars().next().map_or(0, char::len_utf8);
                Self::Formula(Box::new(Formula::from_source(&text[offset..], None, None)?))
            }
            _ => return Err(invalid()),
        };
        Ok(Cow::Owned(owned))
    }
}
