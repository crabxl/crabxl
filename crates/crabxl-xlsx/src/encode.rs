// SPDX-License-Identifier: MIT
// Cell XML layouts adapted from rust_xlsxwriter, Copyright 2022-2026 John McNamara.
// Source provenance and changes: third_party/ports.json.

use crabxl_core::{CellValue, Error, ErrorKind, Result};
use std::io::{self, Write};

pub(crate) struct RowBuffer {
    pub data: Vec<u8>,
    pub maximum: usize,
}
impl Write for RowBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.data.len()) {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Encoded row byte limit exceeded",
            ));
        }
        if self.data.len() + bytes.len() > self.data.capacity() {
            let wanted = (self.data.capacity().saturating_mul(2))
                .max(256)
                .max(self.data.len() + bytes.len())
                .min(self.maximum);
            self.data
                .try_reserve_exact(wanted - self.data.len())
                .map_err(io::Error::other)?;
        }
        self.data.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn validate_text(value: &str, maximum: usize) -> Result<()> {
    if value.len() > maximum || value.chars().count() > 32767 {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            "Text exceeds the configured or worksheet character limit",
        ));
    }
    validate_xml_text(value)?;
    // Preserve inline literal spelling, matching the pinned public reference.
    // Shared-string protection is handled by its separate format codec.
    Ok(())
}

pub(crate) fn validate_xml_text(value: &str) -> Result<()> {
    if value.chars().any(|ch| {
        (ch < ' ' && !matches!(ch, '\t' | '\n' | '\r')) || matches!(ch, '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Text contains a character forbidden by XML 1.0",
        ));
    }
    Ok(())
}

pub(crate) fn encode_cells<'a>(
    buffer: &mut RowBuffer,
    index: crabxl_core::RowIndex,
    cells: impl Iterator<Item = &'a crabxl_core::Cell> + Clone,
    maximum_cell: usize,
    maximum_cells: usize,
    styles: &[crabxl_core::CellStyle],
    epoch: crabxl_core::DateEpoch,
) -> Result<()> {
    buffer.data.clear();
    let mut next_column = 0;
    let mut count = 0;
    for cell in cells.clone() {
        count += 1;
        if count > maximum_cells {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Writer row cell count limit exceeded",
            ));
        }
        if cell.address.row != index || cell.address.column.get() < next_column {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Writer cell coordinates must be ordered within their row",
            )
            .with_cell(cell.address));
        }
        next_column = cell.address.column.get() + 1;
        if cell.style.get() as usize >= styles.len() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Cell references an unregistered style",
            )
            .with_cell(cell.address));
        }
        validate_value(&cell.value, maximum_cell, epoch)
            .map_err(|error| error.with_cell(cell.address))?;
        if let Some(date) = date_value(&cell.value) {
            let expected = if date.kind() == crabxl_core::DateKind::Duration {
                crabxl_core::DateKind::Duration
            } else {
                crabxl_core::DateKind::DateTime
            };
            if cell.style.get() != 0
                && crate::styles::date_format(&styles[cell.style.get() as usize].number_format)
                    != Some(expected)
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Date requires an explicit date/time number format",
                )
                .with_cell(cell.address));
            }
        }
    }
    let result = (|| -> io::Result<()> {
        write!(buffer, "<row r=\"{}\">", index.get() + 1)?;
        for cell in cells {
            let style = if cell.style.get() == 0 {
                date_value(&cell.value).map_or(0, |date| match date.kind() {
                    crabxl_core::DateKind::DateTime => 1,
                    crabxl_core::DateKind::Time => 2,
                    crabxl_core::DateKind::Duration => 3,
                })
            } else {
                cell.style.get()
            };
            write!(buffer, "<c r=\"{}\"", cell.address)?;
            if style != 0 {
                write!(buffer, " s=\"{style}\"")?;
            }
            let (literal, formula) = match &cell.value {
                CellValue::Formula(formula) => (formula.cached(), Some(formula)),
                value => (Some(value), None),
            };
            match literal {
                Some(CellValue::Text(_)) => buffer.write_all(if formula.is_some() {
                    b" t=\"str\""
                } else {
                    b" t=\"inlineStr\""
                })?,
                Some(CellValue::Boolean(_)) => buffer.write_all(b" t=\"b\"")?,
                Some(CellValue::Error(_)) => buffer.write_all(b" t=\"e\"")?,
                _ => {}
            }
            buffer.write_all(b">")?;
            if let Some(formula) = formula {
                buffer.write_all(b"<f>")?;
                write_text(buffer, formula.expression())?;
                buffer.write_all(b"</f>")?;
            }
            match literal {
                None | Some(CellValue::Empty) => {}
                Some(CellValue::Text(value)) if formula.is_none() => {
                    buffer.write_all(b"<is><t xml:space=\"preserve\">")?;
                    write_text(buffer, value.as_str())?;
                    buffer.write_all(b"</t></is>")?;
                }
                Some(value) => {
                    buffer.write_all(b"<v>")?;
                    match value {
                        CellValue::Integer(value) => write!(buffer, "{value}")?,
                        CellValue::BigInteger(value) => {
                            buffer.write_all(value.as_str().as_bytes())?
                        }
                        CellValue::Number(value) => write!(buffer, "{value:?}")?,
                        CellValue::Boolean(value) => write!(buffer, "{}", u8::from(*value))?,
                        CellValue::Text(value) => write_text(buffer, value.as_str())?,
                        CellValue::Error(value) => write_text(buffer, value.as_str())?,
                        CellValue::DateTime(value) => write!(
                            buffer,
                            "{:?}",
                            value.serial_in(epoch).map_err(io::Error::other)?
                        )?,
                        _ => {
                            return Err(io::Error::new(
                                io::ErrorKind::Unsupported,
                                "Unsupported cell value",
                            ));
                        }
                    }
                    buffer.write_all(b"</v>")?;
                }
            }
            buffer.write_all(b"</c>")?;
        }
        buffer.write_all(b"</row>")
    })();
    result.map_err(|error| {
        Error::caused_by(
            ErrorKind::LimitExceeded,
            "Cannot encode worksheet row",
            error,
        )
    })
}
fn date_value(value: &CellValue) -> Option<&crabxl_core::ExcelDateTime> {
    match value {
        CellValue::DateTime(value) => Some(value),
        CellValue::Formula(value) => value.cached().and_then(date_value),
        _ => None,
    }
}
pub(crate) fn validate_value(
    value: &CellValue,
    maximum: usize,
    epoch: crabxl_core::DateEpoch,
) -> Result<()> {
    match value {
        CellValue::Number(number) if !number.is_finite() => Err(Error::new(
            ErrorKind::InvalidData,
            "Cannot write a non-finite number",
        )),
        CellValue::Text(value) => validate_text(value.as_str(), maximum),
        CellValue::Error(value) => {
            if value.as_str().is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Cannot write an empty error token",
                ));
            }
            validate_text(value.as_str(), maximum)
        }
        CellValue::BigInteger(value) if value.as_str().len() > maximum => Err(Error::new(
            ErrorKind::LimitExceeded,
            "Exact integer exceeds writer cell limit",
        )),
        CellValue::DateTime(value) => value.serial_in(epoch).map(|_| ()),
        CellValue::Formula(value) => {
            if value.expression().len() > maximum || value.expression().chars().count() > 8192 {
                return Err(Error::new(
                    ErrorKind::LimitExceeded,
                    "Formula exceeds writer cell limit",
                ));
            }
            validate_xml_text(value.expression())?;
            if let Some(cache) = value.cached() {
                validate_value(cache, maximum, epoch)?;
            }
            Ok(())
        }
        CellValue::Empty
        | CellValue::Number(_)
        | CellValue::Integer(_)
        | CellValue::BigInteger(_)
        | CellValue::Boolean(_) => Ok(()),
        _ => Err(Error::new(ErrorKind::Unsupported, "Unsupported cell value")),
    }
}

fn write_text(output: &mut impl Write, text: &str) -> io::Result<()> {
    let mut start = 0;
    for (offset, ch) in text.char_indices() {
        let replacement = match ch {
            '&' => b"&amp;".as_slice(),
            '<' => b"&lt;".as_slice(),
            '>' => b"&gt;".as_slice(),
            '\r' => b"&#13;".as_slice(),
            _ => continue,
        };
        output.write_all(&text.as_bytes()[start..offset])?;
        output.write_all(replacement)?;
        start = offset + ch.len_utf8();
    }
    output.write_all(&text.as_bytes()[start..])
}
