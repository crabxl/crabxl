// SPDX-License-Identifier: MIT
// Token categories and lexical states adapted from umya-spreadsheet.
// Copyright (c) 2020 MathNya.
// Upstream partly based on Copyright (c) 2007 E. W. Bachtal, Inc.
// Source provenance and semantic changes: third_party/ports.json.
//! Borrowed formula lexing; this is not an expression evaluator.
use crate::{Error, ErrorKind, Result};
use std::borrow::Cow;

/// Lexical role of a formula token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Non-formula input.
    Literal,
    /// Text, number, logical, error or reference.
    Operand,
    /// Function invocation boundary.
    Function,
    /// Array boundary.
    Array,
    /// Grouping boundary.
    Parenthesis,
    /// Function argument or array row separator.
    Separator,
    /// Unary operator.
    Prefix,
    /// Binary or reference union operator.
    Infix,
    /// Percent operator.
    Postfix,
    /// Significant source whitespace.
    Whitespace,
}
/// Additional operand or boundary meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenSubtype {
    /// No additional meaning.
    None,
    /// Quoted literal text.
    Text,
    /// Numeric literal.
    Number,
    /// TRUE or FALSE literal.
    Logical,
    /// Excel error literal.
    Error,
    /// Range, name, structured or external reference.
    Range,
    /// Opening boundary.
    Open,
    /// Closing boundary.
    Close,
    /// Argument separator.
    Argument,
    /// Array row separator.
    Row,
}
impl TokenKind {
    /// Stable lexical category spelling used by compatibility adapters.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Literal => "LITERAL",
            Self::Operand => "OPERAND",
            Self::Function => "FUNC",
            Self::Array => "ARRAY",
            Self::Parenthesis => "PAREN",
            Self::Separator => "SEP",
            Self::Prefix => "OPERATOR-PREFIX",
            Self::Infix => "OPERATOR-INFIX",
            Self::Postfix => "OPERATOR-POSTFIX",
            Self::Whitespace => "WHITE-SPACE",
        }
    }
}
impl TokenSubtype {
    /// Stable operand and boundary spelling used by compatibility adapters.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Text => "TEXT",
            Self::Number => "NUMBER",
            Self::Logical => "LOGICAL",
            Self::Error => "ERROR",
            Self::Range => "RANGE",
            Self::Open => "OPEN",
            Self::Close => "CLOSE",
            Self::Argument => "ARG",
            Self::Row => "ROW",
        }
    }
}
/// A token borrowing its source spelling instead of allocating a string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaToken<'a> {
    /// Source token spelling, including quotation marks and function opener.
    pub value: Cow<'a, str>,
    /// Lexical category.
    pub kind: TokenKind,
    /// Operand or boundary subtype.
    pub subtype: TokenSubtype,
}

/// Tokenize with a joint byte cap on source length and token/stack work capacity.
/// Source input is borrowed. Unclosed function/group openers remain observable,
/// matching the reference lexer; unmatched closers return a typed error.
pub fn tokenize_formula(source: &str, maximum: usize) -> Result<Vec<FormulaToken<'_>>> {
    if source.len() > maximum {
        return Err(budget());
    }
    let mut tokens = Vec::new();
    let mut stack = Vec::<TokenKind>::new();
    if !source.starts_with('=') {
        if !source.is_empty() {
            push(
                &mut tokens,
                FormulaToken {
                    value: Cow::Borrowed(source),
                    kind: TokenKind::Literal,
                    subtype: TokenSubtype::None,
                },
                0,
                maximum,
            )?;
        }
        return Ok(tokens);
    }
    let bytes = source.as_bytes();
    let mut index = 1;
    let mut previous = None;
    let mut payload = 0usize;
    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];
        let mut word = None::<String>;
        let mut segment = start;
        let (kind, mut subtype) = match byte {
            b' ' | b'\n' => {
                index += 1;
                while bytes.get(index) == Some(&byte) {
                    index += 1;
                }
                (TokenKind::Whitespace, TokenSubtype::None)
            }
            b'"' => {
                index = quoted(bytes, index, b'"')?;
                (TokenKind::Operand, TokenSubtype::Text)
            }
            b'#' => {
                let error = [
                    "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A",
                ]
                .into_iter()
                .find(|error| source[index..].starts_with(error))
                .ok_or_else(|| invalid("Unknown formula error literal or spill operator"))?;
                index += error.len();
                (TokenKind::Operand, TokenSubtype::Error)
            }
            b'(' | b'{' => {
                let kind = if byte == b'(' {
                    TokenKind::Parenthesis
                } else {
                    TokenKind::Array
                };
                push(
                    &mut stack,
                    kind,
                    (tokens.capacity() * size_of::<FormulaToken<'_>>())
                        .saturating_add(payload)
                        .saturating_add(word.as_ref().map_or(0, String::capacity)),
                    maximum,
                )?;
                index += 1;
                (kind, TokenSubtype::Open)
            }
            b')' | b'}' => {
                let kind = stack
                    .pop()
                    .ok_or_else(|| invalid("Unmatched formula closer"))?;
                if (byte == b'}') != (kind == TokenKind::Array) {
                    return Err(invalid("Mismatched formula closer"));
                }
                index += 1;
                (kind, TokenSubtype::Close)
            }
            b',' | b';' => {
                index += 1;
                if byte == b';' {
                    (TokenKind::Separator, TokenSubtype::Row)
                } else if matches!(stack.last(), Some(TokenKind::Function | TokenKind::Array)) {
                    (TokenKind::Separator, TokenSubtype::Argument)
                } else {
                    (TokenKind::Infix, TokenSubtype::None)
                }
            }
            b'%' => {
                index += 1;
                (TokenKind::Postfix, TokenSubtype::None)
            }
            b':' => {
                // Literal A1:B2 ranges remain a single operand; a colon after
                // a generated reference is an operator (pending MR !345).
                index += 1;
                (TokenKind::Infix, TokenSubtype::None)
            }
            b'+' | b'-' | b'*' | b'/' | b'^' | b'&' | b'=' | b'<' | b'>' => {
                index += 1;
                if matches!(bytes.get(start..index + 1), Some(b">=" | b"<=" | b"<>")) {
                    index += 1;
                }
                let prefix = matches!(byte, b'+' | b'-')
                    && !matches!(
                        previous,
                        Some((TokenKind::Operand | TokenKind::Postfix, _))
                            | Some((_, TokenSubtype::Close))
                    );
                (
                    if prefix {
                        TokenKind::Prefix
                    } else {
                        TokenKind::Infix
                    },
                    TokenSubtype::None,
                )
            }
            _ => {
                while index < bytes.len() {
                    match bytes[index] {
                        b'\'' => {
                            index = quoted(bytes, index, b'\'')?;
                        }
                        b'[' => {
                            let mut depth = 1usize;
                            index += 1;
                            while index < bytes.len() && depth != 0 {
                                match bytes[index] {
                                    b'[' => depth += 1,
                                    b']' => depth -= 1,
                                    _ => {}
                                }
                                index += 1;
                            }
                            if depth != 0 {
                                return Err(invalid("Unclosed formula bracket"));
                            }
                        }
                        b'+' | b'-'
                            if index > start + 1
                                && matches!(bytes[index - 1], b'e' | b'E')
                                && source[start..index - 1].parse::<f64>().is_ok() =>
                        {
                            index += 1;
                        }
                        b'\n' => {
                            let other = tokens.capacity() * size_of::<FormulaToken<'_>>()
                                + stack.capacity() * size_of::<TokenKind>()
                                + payload;
                            append_word(&mut word, &source[segment..index], other, maximum)?;
                            push(
                                &mut tokens,
                                FormulaToken {
                                    value: Cow::Borrowed("\n"),
                                    kind: TokenKind::Whitespace,
                                    subtype: TokenSubtype::None,
                                },
                                stack.capacity() * size_of::<TokenKind>()
                                    + payload
                                    + word.as_ref().map_or(0, String::capacity),
                                maximum,
                            )?;
                            index += 1;
                            while bytes.get(index) == Some(&b'\n') {
                                index += 1;
                            }
                            segment = index;
                        }
                        b' ' | b'"' | b'#' | b'(' | b')' | b'{' | b'}' | b',' | b';' | b'%'
                        | b'+' | b'-' | b'*' | b'/' | b'^' | b'&' | b'=' | b'<' | b'>' => {
                            break;
                        }
                        _ => {
                            index += 1;
                        }
                    }
                }
                if bytes.get(index) == Some(&b'(') {
                    push(
                        &mut stack,
                        TokenKind::Function,
                        (tokens.capacity() * size_of::<FormulaToken<'_>>())
                            .saturating_add(payload)
                            .saturating_add(word.as_ref().map_or(0, String::capacity)),
                        maximum,
                    )?;
                    index += 1;
                    (TokenKind::Function, TokenSubtype::Open)
                } else {
                    let value = &source[start..index];
                    let subtype = if matches!(value, "TRUE" | "FALSE") {
                        TokenSubtype::Logical
                    } else if value.parse::<f64>().is_ok() {
                        TokenSubtype::Number
                    } else {
                        TokenSubtype::Range
                    };
                    (TokenKind::Operand, subtype)
                }
            }
        };
        let value = if word.is_some() {
            let other = tokens.capacity() * size_of::<FormulaToken<'_>>()
                + stack.capacity() * size_of::<TokenKind>()
                + payload;
            append_word(&mut word, &source[segment..index], other, maximum)?;
            let owned = word.take().ok_or_else(|| invalid("Missing formula word"))?;
            payload = payload.saturating_add(owned.capacity());
            Cow::Owned(owned)
        } else if kind == TokenKind::Whitespace {
            Cow::Borrowed(if byte == b' ' { " " } else { "\n" })
        } else {
            Cow::Borrowed(&source[start..index])
        };
        if kind == TokenKind::Operand
            && !matches!(subtype, TokenSubtype::Text | TokenSubtype::Error)
        {
            subtype = classify_formula_operand(&value);
        }
        push(
            &mut tokens,
            FormulaToken {
                value,
                kind,
                subtype,
            },
            (stack.capacity() * size_of::<TokenKind>()).saturating_add(payload),
            maximum,
        )?;
        if kind != TokenKind::Whitespace {
            previous = Some((kind, subtype));
        }
    }
    Ok(tokens)
}

fn quoted(bytes: &[u8], start: usize, quote: u8) -> Result<usize> {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == quote {
            index += 1;
            if bytes.get(index) != Some(&quote) {
                return Ok(index);
            }
        }
        index += 1;
    }
    Err(invalid("Unclosed formula quotation"))
}

fn push<T>(values: &mut Vec<T>, value: T, other: usize, maximum: usize) -> Result<()> {
    if values.len() == values.capacity() {
        let available = maximum
            .saturating_sub(other)
            .saturating_sub(values.capacity().saturating_mul(size_of::<T>()))
            / size_of::<T>();
        let capacity = values.capacity().saturating_mul(2).max(4).min(available);
        if capacity <= values.len() {
            return Err(budget());
        }
        let work = capacity
            .saturating_add(values.capacity())
            .saturating_mul(size_of::<T>())
            .saturating_add(other);
        if work > maximum {
            return Err(budget());
        }
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| budget())?;
        if values
            .capacity()
            .saturating_mul(size_of::<T>())
            .saturating_add(other)
            > maximum
        {
            return Err(budget());
        }
    }
    values.push(value);
    Ok(())
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Formula token allowance exceeded",
    )
}

/// Classify a public operand literal without evaluating or validating an expression.
pub fn classify_formula_operand(value: &str) -> TokenSubtype {
    if value.starts_with('"') {
        TokenSubtype::Text
    } else if value.starts_with('#') {
        TokenSubtype::Error
    } else if matches!(value, "TRUE" | "FALSE") {
        TokenSubtype::Logical
    } else if value.trim().parse::<f64>().is_ok() {
        TokenSubtype::Number
    } else {
        TokenSubtype::Range
    }
}

fn append_word(
    word: &mut Option<String>,
    segment: &str,
    other: usize,
    maximum: usize,
) -> Result<()> {
    let value = word.get_or_insert_with(String::new);
    let length = value.len().checked_add(segment.len()).ok_or_else(budget)?;
    if length > value.capacity() {
        let available = maximum
            .saturating_sub(other)
            .saturating_sub(value.capacity());
        let capacity = value
            .capacity()
            .saturating_mul(2)
            .max(length)
            .min(available);
        if capacity < length {
            return Err(budget());
        }
        value
            .try_reserve_exact(capacity - value.len())
            .map_err(|_| budget())?;
        if value.capacity().saturating_add(other) > maximum {
            return Err(budget());
        }
    }
    value.push_str(segment);
    Ok(())
}
