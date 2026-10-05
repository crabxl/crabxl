// SPDX-License-Identifier: MIT
// Cell XML layouts adapted from rust_xlsxwriter, Copyright 2022-2026 John McNamara.
// Source provenance and changes: third_party/ports.json.

use crabxl_core::{CellValue, Error, ErrorKind, Result};
use std::io::{self, Write};

pub(crate) enum StyleContext<'a> {
    Registry {
        registry: &'a mut crabxl_core::StyleRegistry,
        maximum: usize,
    },
    Appearance(&'a [crabxl_core::CellStyle]),
}
impl StyleContext<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Registry { registry, .. } => registry.catalog().cell_formats.len(),
            Self::Appearance(styles) => styles.len(),
        }
    }
    fn fonts(&self) -> usize {
        match self {
            Self::Registry { registry, .. } => registry.catalog().fonts.len(),
            Self::Appearance(styles) => styles.len(),
        }
    }
    fn number_format(&self, id: u32) -> Option<&str> {
        match self {
            Self::Registry { registry, .. } => registry
                .catalog()
                .cell_format(crabxl_core::StyleId::new(id))
                .and_then(|format| registry.catalog().number_format(format.number_format_id)),
            Self::Appearance(styles) => styles
                .get(id as usize)
                .map(|style| style.number_format.as_ref()),
        }
    }
    fn prepare_date_format(&mut self, id: u32, date: &crabxl_core::ExcelDateTime) -> Result<()> {
        if id == 0
            || self
                .number_format(id)
                .and_then(crabxl_core::classify_number_format)
                .is_some()
        {
            return Ok(());
        }
        match self {
            Self::Registry { registry, maximum } => {
                registry.register_temporal_format_with_limit(
                    crabxl_core::StyleId::new(id),
                    date.kind(),
                    *maximum,
                )?;
                Ok(())
            }
            Self::Appearance(_) => Err(Error::new(
                ErrorKind::InvalidData,
                "Date requires an explicit date/time number format",
            )),
        }
    }
    fn resolved_style(&self, cell: &crabxl_core::Cell, dates: DateStyleIds) -> Result<u32> {
        let id = cell.style.get();
        let Some(date) = date_value(&cell.value) else {
            return Ok(id);
        };
        if self
            .number_format(id)
            .and_then(crabxl_core::classify_number_format)
            .is_some()
        {
            return Ok(id);
        }
        if id == 0 {
            return Ok(dates.for_kind(date.kind()).get());
        }
        match self {
            Self::Registry { registry, .. } => {
                let number = registry
                    .catalog()
                    .cell_format(dates.for_kind(date.kind()))
                    .ok_or_else(|| {
                        Error::new(ErrorKind::InvalidState, "Date preset is unavailable")
                    })?
                    .number_format_id;
                registry
                    .find_format_with_number_format(cell.style, number)?
                    .map(|style| style.get())
                    .ok_or_else(|| {
                        Error::new(ErrorKind::InvalidState, "Date format was not prepared")
                    })
            }
            Self::Appearance(_) => Ok(id),
        }
    }
}

pub(crate) use crabxl_core::TemporalStyleIds as DateStyleIds;
pub(crate) struct ValueEncoding {
    pub(crate) epoch: crabxl_core::DateEpoch,
    pub(crate) iso_dates: bool,
    pub(crate) non_finite: crate::NonFiniteWritePolicy,
    pub(crate) formula_attributes: crate::FormulaWritePolicy,
    pub(crate) date_styles: DateStyleIds,
}

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
    mut styles: StyleContext<'_>,
    date_encoding: ValueEncoding,
) -> Result<()> {
    let ValueEncoding {
        epoch,
        iso_dates,
        non_finite,
        formula_attributes,
        date_styles,
    } = date_encoding;
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
        let iso_date = iso_dates
            && date_value(&cell.value)
                .is_some_and(|date| date.kind() != crabxl_core::DateKind::Duration);
        let validation_epoch = if iso_date {
            date_value(&cell.value).map_or(epoch, |date| date.epoch())
        } else {
            epoch
        };
        validate_non_finite(&cell.value, non_finite)
            .map_err(|error| error.with_cell(cell.address))?;
        validate_value(&cell.value, maximum_cell, validation_epoch)
            .map_err(|error| error.with_cell(cell.address))?;
        if iso_date && let Some(date) = date_value(&cell.value) {
            validate_text(&date.to_iso8601()?, maximum_cell)
                .map_err(|error| error.with_cell(cell.address))?;
        }
        if let CellValue::RichText(value) = &cell.value
            && value
                .phonetic_properties
                .as_ref()
                .is_some_and(|p| p.font_id as usize >= styles.fonts())
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Phonetic font is not registered in the writer catalog",
            )
            .with_cell(cell.address));
        }
    }
    // Validate every input before interning any derived style. A later encoded
    // byte-limit failure may retain a reusable variant, but commits no row bytes.
    for cell in cells.clone() {
        if let Some(date) = date_value(&cell.value) {
            styles
                .prepare_date_format(cell.style.get(), date)
                .map_err(|error| error.with_cell(cell.address))?;
        }
    }
    let result = (|| -> io::Result<()> {
        write!(buffer, "<row r=\"{}\">", index.get() + 1)?;
        for cell in cells {
            let style = styles
                .resolved_style(cell, date_styles)
                .map_err(io::Error::other)?;
            write!(buffer, "<c r=\"{}\"", cell.address)?;
            if style != 0 {
                write!(buffer, " s=\"{style}\"")?;
            }
            let (literal, formula) = match &cell.value {
                CellValue::Formula(formula) => (formula.cached(), Some(formula)),
                value => (Some(value), None),
            };
            match literal {
                Some(CellValue::Text(_) | CellValue::RichText(_)) => {
                    buffer.write_all(if formula.is_some() {
                        b" t=\"str\""
                    } else {
                        b" t=\"inlineStr\""
                    })?
                }
                Some(CellValue::Boolean(_)) => buffer.write_all(b" t=\"b\"")?,
                Some(CellValue::Error(_)) => buffer.write_all(b" t=\"e\"")?,
                Some(CellValue::DateTime(value))
                    if iso_dates && value.kind() != crabxl_core::DateKind::Duration =>
                {
                    buffer.write_all(b" t=\"d\"")?
                }
                _ => {}
            }
            buffer.write_all(b">")?;
            if let Some(formula) = formula {
                crate::formula_codec::write(buffer, formula, formula_attributes)?;
            }
            match literal {
                None | Some(CellValue::Empty) => {}
                Some(CellValue::Text(value)) if formula.is_none() => {
                    buffer.write_all(b"<is><t xml:space=\"preserve\">")?;
                    write_text(buffer, value.as_str())?;
                    buffer.write_all(b"</t></is>")?;
                }
                Some(CellValue::RichText(value)) if formula.is_none() => {
                    crate::rich_text::write_container(buffer, value)?
                }
                Some(value) => {
                    buffer.write_all(b"<v>")?;
                    match value {
                        CellValue::Integer(value) => write!(buffer, "{value}")?,
                        CellValue::BigInteger(value) => {
                            buffer.write_all(value.as_str().as_bytes())?
                        }
                        CellValue::Number(value) if value.is_finite() => {
                            write!(buffer, "{value:?}")?
                        }
                        CellValue::Number(_) => {}
                        CellValue::Boolean(value) => write!(buffer, "{}", u8::from(*value))?,
                        CellValue::Text(value) => write_text(buffer, value.as_str())?,
                        CellValue::Error(value) => write_text(buffer, value.as_str())?,
                        CellValue::DateTime(value) => {
                            if iso_dates && value.kind() != crabxl_core::DateKind::Duration {
                                buffer.write_all(
                                    value.to_iso8601().map_err(io::Error::other)?.as_bytes(),
                                )?;
                            } else {
                                write!(
                                    buffer,
                                    "{:?}",
                                    value.serial_in(epoch).map_err(io::Error::other)?
                                )?;
                            }
                        }
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
    value.temporal_value()
}
pub(crate) fn validate_non_finite(
    value: &CellValue,
    policy: crate::NonFiniteWritePolicy,
) -> Result<()> {
    if policy == crate::NonFiniteWritePolicy::Reject {
        let literal = match value {
            CellValue::Formula(formula) => formula.cached(),
            other => Some(other),
        };
        if matches!(literal, Some(CellValue::Number(number)) if !number.is_finite()) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Cannot write a non-finite number under Reject policy",
            ));
        }
    }
    Ok(())
}
pub(crate) fn validate_value(
    value: &CellValue,
    maximum: usize,
    epoch: crabxl_core::DateEpoch,
) -> Result<()> {
    match value {
        CellValue::Text(value) => validate_text(value.as_str(), maximum),
        CellValue::RichText(value) => crate::rich_text::validate(value, maximum),
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
            if let Some(metadata) = value.metadata() {
                metadata.validate()?;
                if let crabxl_core::FormulaType::Shared { index, .. } = &metadata.kind
                    && let Some(literal) = index.literal()
                {
                    validate_xml_text(literal)?;
                }
                if let Some(reference) = &metadata.reference {
                    validate_xml_text(&reference.spelling())?;
                }
                for flag in [
                    &metadata.flags.always_calculate,
                    &metadata.flags.calculate_cell,
                    &metadata.flags.data_box,
                ]
                .into_iter()
                .flatten()
                {
                    validate_xml_text(flag.spelling())?;
                }
                if let Some(table) = &metadata.data_table {
                    for flag in [
                        &table.two_dimensions,
                        &table.row_table,
                        &table.deleted1,
                        &table.deleted2,
                    ]
                    .into_iter()
                    .flatten()
                    {
                        validate_xml_text(flag.spelling())?;
                    }
                    for input in [&table.input1, &table.input2].into_iter().flatten() {
                        validate_xml_text(input)?;
                    }
                }
                if metadata.payload_bytes() > maximum {
                    return Err(Error::new(
                        ErrorKind::LimitExceeded,
                        "Formula metadata exceeds writer cell limit",
                    ));
                }
            }
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

pub(crate) fn write_attribute(output: &mut impl Write, name: &str, value: &str) -> io::Result<()> {
    write!(output, " {name}=\"")?;
    write_xml(output, value, true)?;
    output.write_all(b"\"")
}
pub(crate) fn write_text(output: &mut impl Write, text: &str) -> io::Result<()> {
    write_xml(output, text, false)
}
fn write_xml(output: &mut impl Write, text: &str, attribute: bool) -> io::Result<()> {
    let mut start = 0;
    for (offset, ch) in text.char_indices() {
        let replacement = match ch {
            '&' => b"&amp;".as_slice(),
            '<' => b"&lt;".as_slice(),
            '>' => b"&gt;".as_slice(),
            '\r' => b"&#13;".as_slice(),
            '\n' if attribute => b"&#10;".as_slice(),
            '\t' if attribute => b"&#9;".as_slice(),
            '\"' if attribute => b"&quot;".as_slice(),
            '\'' if attribute => b"&apos;".as_slice(),
            _ => continue,
        };
        output.write_all(&text.as_bytes()[start..offset])?;
        output.write_all(replacement)?;
        start = offset + ch.len_utf8();
    }
    output.write_all(&text.as_bytes()[start..])
}
