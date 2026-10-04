// SPDX-License-Identifier: MIT
// Reference/offset scanning adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source, behavior changes and tests: third_party/ports.json.
//! Bounded, runtime-independent A1 expression translation; not a calculation engine.
use crate::{Error, ErrorKind, Result};
use std::fmt::{self, Write};

#[derive(Clone, Copy)]
struct Reference<'a> {
    column: Option<(&'a str, u32, bool)>,
    row: Option<(u64, bool)>,
}
impl<'a> Reference<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        Self::parse_with_row_digits(text, 7)
    }
    fn parse_with_row_digits(text: &'a str, maximum: usize) -> Option<Self> {
        let bytes = text.as_bytes();
        let mut at = usize::from(bytes.first() == Some(&b'$'));
        let initial_absolute = at != 0;
        let start = at;
        let mut column = 0u32;
        while let Some(letter) = bytes.get(at).filter(|letter| letter.is_ascii_alphabetic()) {
            column = column
                .checked_mul(26)?
                .checked_add(u32::from(letter.to_ascii_uppercase() - b'A' + 1))?;
            at += 1;
        }
        if at - start > 3 {
            return None;
        }
        let column = (at != start).then_some((&text[start..at], column, initial_absolute));
        let row_absolute = if column.is_none() {
            initial_absolute
        } else if bytes.get(at) == Some(&b'$') {
            at += 1;
            true
        } else {
            false
        };
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at != bytes.len() || at - start > maximum {
            return None;
        }
        let row = if at == start {
            None
        } else {
            if bytes[start] == b'0' {
                return None;
            }
            Some((text[start..].parse().ok()?, row_absolute))
        };
        if (column.is_none() && row.is_none())
            || (column.is_some() && row.is_none() && row_absolute)
        {
            return None;
        }
        Some(Self { column, row })
    }
    fn shifted(self, rows: i64, columns: i64, output: &mut LimitedString) -> Result<()> {
        if let Some((original, value, absolute)) = self.column {
            if absolute {
                output.add("$")?;
                output.add(original)?;
            } else {
                let column = i128::from(value) + i128::from(columns);
                // openpyxl's formula translator uses its three-letter utility
                // limit, rather than the physical XFD worksheet boundary.
                if !(1..=18_278).contains(&column) {
                    return Err(out_of_range());
                }
                crate::address::write_column_name(output, (column - 1) as u32)
                    .map_err(|_| allowance())?;
            }
        }
        if let Some((value, absolute)) = self.row {
            let row = if absolute {
                i128::from(value)
            } else {
                i128::from(value) + i128::from(rows)
            };
            if row < 1 || row > i128::from(u64::MAX) {
                return Err(out_of_range());
            }
            if absolute {
                output.add("$")?;
            }
            write!(output, "{row}").map_err(|_| allowance())?;
        }
        Ok(())
    }
}
struct LimitedString {
    data: String,
    maximum: usize,
}
impl LimitedString {
    fn add(&mut self, text: &str) -> Result<()> {
        self.write_str(text).map_err(|_| allowance())
    }
}
impl fmt::Write for LimitedString {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let required = self.data.len().checked_add(text.len()).ok_or(fmt::Error)?;
        if required > self.maximum {
            return Err(fmt::Error);
        }
        if required > self.data.capacity() {
            let target = required
                .max(self.data.capacity().saturating_mul(2))
                .min(self.maximum);
            self.data
                .try_reserve_exact(target - self.data.len())
                .map_err(|_| fmt::Error)?;
        }
        self.data.push_str(text);
        Ok(())
    }
}
fn allowance() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Formula translation output allowance exceeded",
    )
}
fn out_of_range() -> Error {
    Error::new(
        ErrorKind::InvalidData,
        "Formula reference translation is out of range",
    )
}
fn token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || byte >= 128
        || matches!(byte, b'_' | b'.' | b'\\' | b'$' | b':' | b'@')
}
fn shifted_token(token: &str, rows: i64, cols: i64, output: &mut LimitedString) -> Result<()> {
    let all_columns = token.bytes().filter(|byte| *byte == b':').count() == 1
        && token
            .split(':')
            .all(|part| Reference::parse(part).is_some_and(|reference| reference.row.is_none()));
    let all_rows = token.bytes().filter(|byte| *byte == b':').count() == 1
        && token
            .split(':')
            .all(|part| Reference::parse(part).is_some_and(|reference| reference.column.is_none()));
    for (index, part) in token.split(':').enumerate() {
        if index != 0 {
            output.add(":")?;
        }
        match Reference::parse(part) {
            Some(reference)
                if (reference.column.is_some() && reference.row.is_some())
                    || all_columns
                    || all_rows =>
            {
                reference.shifted(rows, cols, output)?
            }
            _ => output.add(part)?,
        }
    }
    Ok(())
}
/// Shift relative A1 references in a bare or equals-prefixed expression.
/// Absolute axes, sheet qualifiers, quoted strings and structured-reference
/// brackets retain their context. Whole-row/column ranges do not expand cells.
/// Output capacity is byte-bounded; input and caller data are additional.
/// This is a translation scanner, not a complete formula tokenizer/parser.
/// Dynamic spill syntax and unclosed quotes/brackets are rejected explicitly.
/// Reference row labels beyond seven digits retain baseline named-range behavior.
pub fn translate_expression(
    expression: &str,
    rows: i64,
    columns: i64,
    max_bytes: usize,
) -> Result<String> {
    if expression.len() > max_bytes {
        return Err(allowance());
    }
    let mut output = LimitedString {
        data: String::new(),
        maximum: max_bytes,
    };
    output
        .data
        .try_reserve_exact(expression.len())
        .map_err(|_| allowance())?;
    let bytes = expression.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        let start = at;
        if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == quote {
                    at += 1;
                    if bytes.get(at) == Some(&quote) {
                        at += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    at += 1;
                }
            }
            if !closed {
                return Err(Error::new(ErrorKind::InvalidData, "Unclosed formula quote"));
            }
            output.add(&expression[start..at])?;
        } else if bytes[at] == b'[' {
            let mut depth = 1usize;
            at += 1;
            while at < bytes.len() && depth != 0 {
                if bytes[at] == b'[' {
                    depth += 1;
                } else if bytes[at] == b']' {
                    depth -= 1;
                }
                at += 1;
            }
            if depth != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unclosed formula bracket",
                ));
            }
            output.add(&expression[start..at])?;
        } else if token_byte(bytes[at]) {
            while at < bytes.len() && token_byte(bytes[at]) {
                at += 1;
            }
            if matches!(bytes.get(at), Some(b'(' | b'!' | b'[')) {
                output.add(&expression[start..at])?;
            } else {
                shifted_token(&expression[start..at], rows, columns, &mut output)?;
            }
        } else if bytes[at] == b'#' {
            let error = [
                "#NULL!",
                "#DIV/0!",
                "#VALUE!",
                "#REF!",
                "#NAME?",
                "#NUM!",
                "#N/A",
                "#GETTING_DATA",
            ]
            .into_iter()
            .find(|error| expression[at..].starts_with(error));
            if let Some(error) = error {
                output.add(error)?;
                at += error.len();
            } else {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Dynamic spill or unknown formula error syntax is not supported by translation",
                ));
            }
        } else {
            at += 1;
            output.add(&expression[start..at])?;
        }
    }
    Ok(output.data)
}

/// Parse a formula tool's one-based origin/destination. Unlike a physical cell,
/// its row can extend beyond XLSX worksheet limits, matching baseline utilities.
/// Column labels are limited to three letters; row labels must fit u64.
pub fn formula_position(reference: &str) -> Result<(u64, u32)> {
    let value = Reference::parse_with_row_digits(reference, usize::MAX)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid formula position"))?;
    let row = value
        .row
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Formula position has no row"))?
        .0;
    let column = value
        .column
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Formula position has no column"))?
        .1;
    Ok((row, column))
}

/// Translate one validated positive row or three-letter column label.
/// Absolute labels retain their spelling; relative labels use canonical output.
pub fn translate_axis(reference: &str, delta: i64, row: bool) -> Result<String> {
    let value = Reference::parse_with_row_digits(reference, usize::MAX)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid formula axis label"))?;
    if (row && value.column.is_some()) || (!row && value.row.is_some()) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Formula axis kind mismatch",
        ));
    }
    let mut output = LimitedString {
        data: String::new(),
        maximum: reference.len().saturating_add(24),
    };
    value.shifted(
        if row { delta } else { 0 },
        if row { 0 } else { delta },
        &mut output,
    )?;
    Ok(output.data)
}
