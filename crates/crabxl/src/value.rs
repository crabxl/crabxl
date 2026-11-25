//! Cell values and formulas.

use crate::{Error, ErrorKind, Result};
use crabxl_core::{
    CellError, CellText, CellValue, DateKind, ExactDecimal, ExactInteger, ExcelDateTime,
};
use std::fmt;

/// The value a cell holds. For a formula cell, this is the cached result.
///
/// Values never become formulas: text that starts with `=` stays text. Use
/// [`Formula`] to write a formula.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum Value {
    /// No value: an empty cell, or a formula without a cached result.
    #[default]
    Blank,
    /// `TRUE` or `FALSE`.
    Bool(bool),
    /// A number. See [`Number`] for its exactness guarantees.
    Number(Number),
    /// Text, with whitespace kept.
    Text(Text),
    /// Text with formatted runs.
    RichText(RichText),
    /// A date, date and time, time of day or duration. The cell's number
    /// format decides which one.
    Temporal(Temporal),
    /// A cell error such as `#DIV/0!`.
    Error(ErrorValue),
}

impl Value {
    /// Whether the value is [`Value::Blank`].
    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Blank)
    }
    /// The number as `f64`, for numbers only.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(number) => Some(number.to_f64()),
            _ => None,
        }
    }
    /// The number as `i64`, for integral numbers only.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(number) => number.to_i64(),
            _ => None,
        }
    }
    /// The text, for plain text only.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            _ => None,
        }
    }
    /// The boolean, for booleans only.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
    /// A short name of the variant, for messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::Text(_) => "text",
            Self::RichText(_) => "rich text",
            Self::Temporal(_) => "date or time",
            Self::Error(_) => "error",
        }
    }
}

/// A number, kept without loss of precision.
///
/// Excel stores numbers as IEEE-754 doubles, and [`Number::to_f64`] gives
/// that value. Integers outside the `f64` range and decimals with more digits
/// than `f64` holds keep their exact text, which [`Number::to_exact_string`]
/// returns and which a save writes back unchanged.
#[derive(Clone)]
pub struct Number(NumberRepr);

impl fmt::Debug for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Number({})", self.to_exact_string())
    }
}

#[derive(Clone, Debug)]
enum NumberRepr {
    Integer(i64),
    Float(f64),
    BigInteger(Box<ExactInteger>),
    Decimal(Box<ExactDecimal>),
}

impl Number {
    /// A number from its exact decimal text, such as `"0.1"` or
    /// `"12345678901234567890123"`.
    pub fn exact(text: &str) -> Result<Self> {
        if !text.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
            return Ok(match text.parse::<i64>() {
                Ok(value) => Self(NumberRepr::Integer(value)),
                Err(_) => Self(NumberRepr::BigInteger(Box::new(ExactInteger::parse(text)?))),
            });
        }
        Ok(Self(NumberRepr::Decimal(Box::new(ExactDecimal::parse(
            text,
        )?))))
    }
    /// The nearest `f64`.
    pub fn to_f64(&self) -> f64 {
        match &self.0 {
            NumberRepr::Integer(value) => *value as f64,
            NumberRepr::Float(value) => *value,
            NumberRepr::BigInteger(value) => value.as_str().parse().unwrap_or(f64::NAN),
            NumberRepr::Decimal(value) => value.to_f64(),
        }
    }
    /// The value as `i64` when it is a whole number within range.
    pub fn to_i64(&self) -> Option<i64> {
        match &self.0 {
            NumberRepr::Integer(value) => Some(*value),
            NumberRepr::Float(value) if value.fract() == 0.0 && value.abs() < 9.0e15 => {
                Some(*value as i64)
            }
            _ => None,
        }
    }
    /// The decimal text of an integer stored as an integer, of any size.
    /// `None` for numbers stored as `f64` or as exact decimals.
    pub fn to_integer_string(&self) -> Option<String> {
        match &self.0 {
            NumberRepr::Integer(value) => Some(value.to_string()),
            NumberRepr::BigInteger(value) => Some(value.as_str().to_owned()),
            _ => None,
        }
    }
    /// Whether the number is a whole number.
    pub fn is_integer(&self) -> bool {
        match &self.0 {
            NumberRepr::Integer(_) | NumberRepr::BigInteger(_) => true,
            NumberRepr::Float(value) => value.fract() == 0.0,
            NumberRepr::Decimal(value) => value.to_f64().fract() == 0.0,
        }
    }
    /// The exact decimal text: the file's text for exact numbers, and the
    /// shortest text that reads back as the same `f64` otherwise.
    pub fn to_exact_string(&self) -> String {
        match &self.0 {
            NumberRepr::Integer(value) => value.to_string(),
            NumberRepr::Float(value) => format_float(*value),
            NumberRepr::BigInteger(value) => value.as_str().to_owned(),
            NumberRepr::Decimal(value) => value.as_str().to_owned(),
        }
    }
    pub(crate) fn into_core(self) -> CellValue {
        match self.0 {
            NumberRepr::Integer(value) => CellValue::Integer(value),
            NumberRepr::Float(value) => CellValue::Number(value),
            NumberRepr::BigInteger(value) => CellValue::BigInteger(value),
            NumberRepr::Decimal(value) => CellValue::Decimal(value),
        }
    }
}

fn format_float(value: f64) -> String {
    let text = format!("{value:?}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (NumberRepr::Integer(a), NumberRepr::Integer(b)) => a == b,
            _ => {
                self.to_exact_string() == other.to_exact_string() || self.to_f64() == other.to_f64()
            }
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_exact_string())
    }
}

/// Cell text. Cloning shares the text of large shared-string tables where
/// the reader allows it.
#[derive(Clone, PartialEq, Eq)]
pub struct Text(CellText);

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Text({:?})", self.as_str())
    }
}

impl Text {
    /// The text.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Self(CellText::new(text))
    }
}
impl From<String> for Text {
    fn from(text: String) -> Self {
        Self(CellText::new(text))
    }
}

/// Text with formatted runs and optional phonetic hints.
pub use crabxl_core::RichText;
/// One run of rich text.
pub use crabxl_core::RichTextRun as TextRun;

/// The kind of a [`Temporal`] value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TemporalKind {
    /// A calendar date.
    Date,
    /// A date and a time of day.
    DateTime,
    /// A time of day.
    Time,
    /// A length of time.
    Duration,
}

/// A date, date and time, time of day or duration.
///
/// The value keeps the exact serial number from the file, including serials
/// that have no calendar date (such as Excel's 1900-02-29).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Temporal(ExcelDateTime);

impl Temporal {
    /// A calendar date.
    pub fn date(year: i32, month: u32, day: u32) -> Result<Self> {
        ExcelDateTime::from_ymd(year, month, day).map(Self)
    }
    /// A date and time of day, with microseconds.
    #[allow(clippy::too_many_arguments)]
    pub fn datetime(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        microsecond: u32,
    ) -> Result<Self> {
        ExcelDateTime::from_ymd_hms_micro(year, month, day, hour, minute, second, microsecond)
            .map(Self)
    }
    /// A time of day, with microseconds.
    pub fn time(hour: u32, minute: u32, second: u32, microsecond: u32) -> Result<Self> {
        ExcelDateTime::from_hms_micro(hour, minute, second, microsecond).map(Self)
    }
    /// A duration of whole days, seconds and microseconds.
    pub fn duration(days: i64, seconds: u32, microseconds: u32) -> Result<Self> {
        ExcelDateTime::from_duration_parts(days, seconds, microseconds).map(Self)
    }
    /// The kind of value.
    pub fn kind(&self) -> TemporalKind {
        match self.0.kind() {
            DateKind::Date => TemporalKind::Date,
            DateKind::DateTime => TemporalKind::DateTime,
            DateKind::Time => TemporalKind::Time,
            DateKind::Duration => TemporalKind::Duration,
        }
    }
    /// The number format a cell gets when it receives this value and has no
    /// number format yet.
    pub fn default_format(&self) -> &'static str {
        match self.kind() {
            TemporalKind::Date => "yyyy-mm-dd",
            TemporalKind::DateTime => "yyyy-mm-dd hh:mm:ss",
            TemporalKind::Time => "hh:mm:ss",
            TemporalKind::Duration => "[h]:mm:ss",
        }
    }
    /// The serial number in the workbook's date system.
    pub fn serial(&self) -> f64 {
        self.0.serial()
    }
    /// ISO 8601 text, such as `2026-11-25`, `2026-11-25T09:30:00` or `PT1H`.
    /// Fails for serials without a calendar date.
    pub fn to_iso8601(&self) -> Result<String> {
        self.0.to_iso8601()
    }
    /// The calendar date and time of day: year, month, day, hour, minute,
    /// second and microsecond. Fails for times of day (see
    /// [`Temporal::to_time_of_day`]), durations and serials without a
    /// calendar date.
    pub fn to_parts(&self) -> Result<(i32, u32, u32, u32, u32, u32, u32)> {
        use chrono::{Datelike, Timelike};
        if self.kind() == TemporalKind::Duration {
            return Err(Error::new(
                ErrorKind::Conversion,
                "A duration has no calendar date",
            ));
        }
        let value = self.0.to_datetime()?;
        Ok((
            value.year(),
            value.month(),
            value.day(),
            value.hour(),
            value.minute(),
            value.second(),
            value.nanosecond() / 1000,
        ))
    }
    /// The time of day: hour, minute, second and microsecond. Works for times
    /// and for the time part of dates and times.
    pub fn to_time_of_day(&self) -> Result<(u32, u32, u32, u32)> {
        use chrono::Timelike;
        let time = match self.kind() {
            TemporalKind::Time => self.0.to_time()?,
            TemporalKind::Duration => {
                return Err(Error::new(
                    ErrorKind::Conversion,
                    "A duration is not a time of day",
                ));
            }
            _ => self.0.to_datetime()?.time(),
        };
        Ok((
            time.hour(),
            time.minute(),
            time.second(),
            time.nanosecond() / 1000,
        ))
    }
    /// The total length of a duration in microseconds.
    pub fn to_duration_micros(&self) -> Result<i64> {
        let duration = self.0.to_duration()?;
        duration
            .num_microseconds()
            .ok_or_else(|| Error::new(ErrorKind::Conversion, "The duration is too long"))
    }
}

/// A cell error value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorValue {
    /// `#NULL!`
    Null,
    /// `#DIV/0!`
    Div0,
    /// `#VALUE!`
    Value,
    /// `#REF!`
    Ref,
    /// `#NAME?`
    Name,
    /// `#NUM!`
    Num,
    /// `#N/A`
    NA,
    /// `#GETTING_DATA`
    GettingData,
    /// `#SPILL!`
    Spill,
    /// `#CALC!`
    Calc,
    /// Another error code, kept as written.
    Other(Box<str>),
}

impl ErrorValue {
    /// The error code as Excel shows it.
    pub fn code(&self) -> &str {
        match self {
            Self::Null => "#NULL!",
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Name => "#NAME?",
            Self::Num => "#NUM!",
            Self::NA => "#N/A",
            Self::GettingData => "#GETTING_DATA",
            Self::Spill => "#SPILL!",
            Self::Calc => "#CALC!",
            Self::Other(code) => code,
        }
    }
    /// Parse an error code. Unknown codes that start with `#` are kept.
    pub fn parse(code: &str) -> Result<Self> {
        Ok(match code {
            "#NULL!" => Self::Null,
            "#DIV/0!" => Self::Div0,
            "#VALUE!" => Self::Value,
            "#REF!" => Self::Ref,
            "#NAME?" => Self::Name,
            "#NUM!" => Self::Num,
            "#N/A" => Self::NA,
            "#GETTING_DATA" => Self::GettingData,
            "#SPILL!" => Self::Spill,
            "#CALC!" => Self::Calc,
            other if other.starts_with('#') && other.len() > 1 => Self::Other(other.into()),
            other => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("'{other}' is not an error code"),
                ));
            }
        })
    }
}
impl fmt::Display for ErrorValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// A formula, stored without its leading `=`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Formula {
    text: Box<str>,
}

impl Formula {
    /// A formula from its text, with or without the leading `=`. The text
    /// must be a well-formed expression: quotes, brackets and parentheses
    /// must close.
    pub fn new(text: impl AsRef<str>) -> Result<Self> {
        let text = text.as_ref();
        let body = text.strip_prefix('=').unwrap_or(text);
        if body.trim().is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A formula cannot be empty",
            ));
        }
        let source = format!("={body}");
        let tokens =
            crabxl_core::tokenize_formula(&source, source.len().saturating_mul(4).max(4096))
                .map_err(|error| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Invalid formula '{text}': {}", error.message()),
                    )
                })?;
        let mut depth = 0i32;
        for token in &tokens {
            match token.subtype {
                crabxl_core::TokenSubtype::Open => depth += 1,
                crabxl_core::TokenSubtype::Close => depth -= 1,
                _ => {}
            }
        }
        if depth != 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid formula '{text}': unclosed parenthesis"),
            ));
        }
        Ok(Self { text: body.into() })
    }
    /// The formula text without the leading `=`.
    pub fn text(&self) -> &str {
        &self.text
    }
}
impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "={}", self.text)
    }
}

/// What a cell write accepts: a value or a formula.
#[derive(Clone, Debug, PartialEq)]
pub enum CellInput {
    /// A literal value.
    Value(Value),
    /// A formula; the cell has no cached result until it is calculated.
    Formula(Formula),
}

/// Converts into a [`CellInput`]. Implemented for [`Value`], [`Formula`],
/// numbers, `bool`, text, [`Temporal`], [`ErrorValue`] and `Option<T>`
/// (`None` writes a blank cell). Implement it for your own types to write
/// them directly.
pub trait IntoCell {
    /// Convert the value.
    fn into_cell(self) -> CellInput;
}
impl<T: Into<Value>> IntoCell for T {
    fn into_cell(self) -> CellInput {
        CellInput::Value(self.into())
    }
}
impl IntoCell for Formula {
    fn into_cell(self) -> CellInput {
        CellInput::Formula(self)
    }
}
impl IntoCell for CellInput {
    fn into_cell(self) -> CellInput {
        self
    }
}

macro_rules! value_from {
    ($($ty:ty => |$v:ident| $body:expr),+ $(,)?) => {$(
        impl From<$ty> for Value {
            fn from($v: $ty) -> Self { $body }
        }
    )+};
}
value_from! {
    bool => |v| Value::Bool(v),
    i8 => |v| Value::Number(Number(NumberRepr::Integer(i64::from(v)))),
    i16 => |v| Value::Number(Number(NumberRepr::Integer(i64::from(v)))),
    i32 => |v| Value::Number(Number(NumberRepr::Integer(i64::from(v)))),
    i64 => |v| Value::Number(Number(NumberRepr::Integer(v))),
    u8 => |v| Value::Number(Number(NumberRepr::Integer(i64::from(v)))),
    u16 => |v| Value::Number(Number(NumberRepr::Integer(i64::from(v)))),
    u32 => |v| Value::Number(Number(NumberRepr::Integer(i64::from(v)))),
    u64 => |v| match i64::try_from(v) {
        Ok(v) => Value::Number(Number(NumberRepr::Integer(v))),
        Err(_) => Value::Number(Number(NumberRepr::BigInteger(Box::new(
            ExactInteger::parse(&v.to_string()).unwrap_or_else(|_| unreachable_integer()),
        )))),
    },
    f32 => |v| Value::Number(Number(NumberRepr::Float(f64::from(v)))),
    f64 => |v| Value::Number(Number(NumberRepr::Float(v))),
    &str => |v| Value::Text(Text(CellText::new(v))),
    String => |v| Value::Text(Text(CellText::new(v))),
    &String => |v| Value::Text(Text(CellText::new(v.as_str()))),
    Text => |v| Value::Text(v),
    Number => |v| Value::Number(v),
    Temporal => |v| Value::Temporal(v),
    ErrorValue => |v| Value::Error(v),
    RichText => |v| Value::RichText(v),
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Value::Blank, Into::into)
    }
}

fn unreachable_integer() -> ExactInteger {
    // u64 decimal text is always a valid exact integer.
    ExactInteger::parse("0").unwrap_or_else(|_| unreachable!())
}

/// Converts from a cell [`Value`]. Implemented for `f64`, `i64`, `u32`,
/// `bool`, `String`, [`Number`], [`Temporal`], [`Value`] and `Option<T>`
/// (blank cells become `None`). A value of another type fails with
/// `ErrorKind::Conversion`.
pub trait FromValue: Sized {
    /// Convert the value.
    fn from_value(value: &Value) -> Result<Self>;
}

fn mismatch(expected: &str, value: &Value) -> Error {
    Error::new(
        ErrorKind::Conversion,
        format!("Expected {expected}, found {}", value.type_name()),
    )
}

impl FromValue for Value {
    fn from_value(value: &Value) -> Result<Self> {
        Ok(value.clone())
    }
}
impl FromValue for f64 {
    fn from_value(value: &Value) -> Result<Self> {
        value.as_f64().ok_or_else(|| mismatch("a number", value))
    }
}
impl FromValue for i64 {
    fn from_value(value: &Value) -> Result<Self> {
        value
            .as_i64()
            .ok_or_else(|| mismatch("a whole number", value))
    }
}
impl FromValue for u32 {
    fn from_value(value: &Value) -> Result<Self> {
        value
            .as_i64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| mismatch("a whole number from 0 to 4294967295", value))
    }
}
impl FromValue for bool {
    fn from_value(value: &Value) -> Result<Self> {
        value.as_bool().ok_or_else(|| mismatch("a boolean", value))
    }
}
impl FromValue for String {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Text(text) => Ok(text.as_str().to_owned()),
            Value::RichText(text) => text.plain_text().map(String::from),
            other => Err(mismatch("text", other)),
        }
    }
}
impl FromValue for Number {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Number(number) => Ok(number.clone()),
            other => Err(mismatch("a number", other)),
        }
    }
}
impl FromValue for Temporal {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Temporal(temporal) => Ok(*temporal),
            other => Err(mismatch("a date or time", other)),
        }
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Value) -> Result<Self> {
        if value.is_blank() {
            Ok(None)
        } else {
            T::from_value(value).map(Some)
        }
    }
}
impl TryFrom<&Value> for f64 {
    type Error = Error;
    fn try_from(value: &Value) -> Result<Self> {
        Self::from_value(value)
    }
}
impl TryFrom<&Value> for i64 {
    type Error = Error;
    fn try_from(value: &Value) -> Result<Self> {
        Self::from_value(value)
    }
}
impl TryFrom<&Value> for String {
    type Error = Error;
    fn try_from(value: &Value) -> Result<Self> {
        Self::from_value(value)
    }
}
impl TryFrom<&Value> for bool {
    type Error = Error;
    fn try_from(value: &Value) -> Result<Self> {
        Self::from_value(value)
    }
}

/// Split an engine value into its value (the cached result for formulas)
/// and its formula.
pub(crate) fn from_core(value: CellValue) -> (Value, Option<Formula>) {
    match value {
        CellValue::Empty => (Value::Blank, None),
        CellValue::Boolean(v) => (Value::Bool(v), None),
        CellValue::Integer(v) => (Value::Number(Number(NumberRepr::Integer(v))), None),
        CellValue::Number(v) => (Value::Number(Number(NumberRepr::Float(v))), None),
        CellValue::BigInteger(v) => (Value::Number(Number(NumberRepr::BigInteger(v))), None),
        CellValue::Decimal(v) => (Value::Number(Number(NumberRepr::Decimal(v))), None),
        CellValue::Text(v) => (Value::Text(Text(*v)), None),
        CellValue::Error(v) => (
            Value::Error(
                ErrorValue::parse(v.as_str())
                    .unwrap_or_else(|_| ErrorValue::Other(v.as_str().into())),
            ),
            None,
        ),
        CellValue::RichText(v) => (Value::RichText(*v), None),
        CellValue::DateTime(v) => (Value::Temporal(Temporal(*v)), None),
        CellValue::Formula(formula) => {
            let cached = formula
                .cached()
                .cloned()
                .map_or(Value::Blank, |cached| from_core(cached).0);
            let text = formula.expression();
            (
                cached,
                Some(Formula {
                    text: text.strip_prefix('=').unwrap_or(text).into(),
                }),
            )
        }
        CellValue::Annotated(annotated) => from_core(annotated.value),
        _ => (Value::Blank, None),
    }
}

/// The engine value for a cell input. NaN and infinity are rejected because
/// XLSX cannot store them.
pub(crate) fn to_core(input: CellInput) -> Result<CellValue> {
    Ok(match input {
        CellInput::Formula(formula) => {
            CellValue::Formula(Box::new(crabxl_core::Formula::new(formula.text, None)?))
        }
        CellInput::Value(value) => match value {
            Value::Blank => CellValue::Empty,
            Value::Bool(v) => CellValue::Boolean(v),
            Value::Number(number) => {
                let core = number.into_core();
                if let CellValue::Number(v) = core
                    && !v.is_finite()
                {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "XLSX cannot store NaN or infinity",
                    ));
                }
                core
            }
            Value::Text(text) => {
                if text.as_str().chars().count() > 32_767 {
                    return Err(Error::new(
                        ErrorKind::LimitExceeded,
                        "A cell holds at most 32,767 characters",
                    ));
                }
                CellValue::Text(Box::new(text.0))
            }
            Value::RichText(text) => CellValue::RichText(Box::new(text)),
            Value::Temporal(temporal) => CellValue::DateTime(Box::new(temporal.0)),
            Value::Error(error) => CellValue::Error(Box::new(CellError::new(error.code()))),
        },
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn values_round_trip_through_the_engine() {
        for value in [
            Value::from(42),
            Value::from(0.1),
            Value::from("=not a formula"),
            Value::from(true),
            Value::Number(Number::exact("3.14159265358979323846").unwrap()),
            Value::Number(Number::exact("123456789012345678901234567890").unwrap()),
            Value::Temporal(Temporal::date(2026, 11, 25).unwrap()),
            Value::Error(ErrorValue::Div0),
        ] {
            let core = to_core(CellInput::Value(value.clone())).unwrap();
            assert_eq!(from_core(core).0, value);
        }
        assert_eq!(
            Number::exact("3.14159265358979323846")
                .unwrap()
                .to_exact_string(),
            "3.14159265358979323846"
        );
        assert_eq!(Value::from(1.5).as_f64(), Some(1.5));
        assert_eq!(
            f64::from_value(&Value::from("x")).unwrap_err().kind(),
            ErrorKind::Conversion
        );
        assert!(to_core(CellInput::Value(Value::from(f64::NAN))).is_err());
        assert_eq!(Formula::new("=SUM(A1:A3)").unwrap().text(), "SUM(A1:A3)");
        assert!(Formula::new("=SUM(A1").is_err());
        assert!(Formula::new("").is_err());
        let date = Temporal::datetime(2026, 11, 25, 9, 30, 0, 0).unwrap();
        assert_eq!(date.to_parts().unwrap(), (2026, 11, 25, 9, 30, 0, 0));
    }
}
