// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Values operations.
use super::*;

impl<'a, R: Read + Seek> Rows<'a, R> {
    pub(super) fn read_value<const BOOLEAN: bool>(
        &mut self,
        kind: ScalarKind,
    ) -> Result<CellValue> {
        self.value_buffer.clear();
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)) => {
                    append_xml_text(&mut self.value_buffer, &event, self.limits.max_cell_bytes)?;
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && (frame.depth == 4
                            || (matches!(kind, ScalarKind::InlineText) && frame.depth == 5))
                        && (e.local_name().as_ref().as_bytes() == b"v"
                            || e.local_name().as_ref().as_bytes() == b"t"
                            || e.local_name().as_ref().as_bytes() == b"f") =>
                {
                    break;
                }
                Event::Comment(_) | Event::PI(_) => {}
                _ => return Err(self.invalid("Invalid scalar value content")),
            }
        }
        if matches!(kind, ScalarKind::Text | ScalarKind::InlineText) {
            return Ok(
                if self.value_buffer.is_empty() && matches!(kind, ScalarKind::Text) {
                    CellValue::Empty
                } else {
                    CellValue::text(self.value_buffer.as_str())
                },
            );
        }
        if matches!(kind, ScalarKind::Error) {
            return Ok(if self.value_buffer.is_empty() {
                CellValue::Empty
            } else {
                CellValue::error(self.value_buffer.as_str())
            });
        }
        if matches!(kind, ScalarKind::SharedText) {
            return self.shared_string_value(shared_string_id(self.value_buffer.trim_ascii())?);
        }
        if matches!(kind, ScalarKind::IsoDate) {
            return Ok(crabxl_core::parse_iso8601(&self.value_buffer)?
                .map_or(CellValue::Empty, |v| CellValue::DateTime(Box::new(v))));
        }
        let value = self.value_buffer.trim_ascii();
        if value.is_empty() {
            return Ok(CellValue::Empty);
        }
        if BOOLEAN {
            return boolean_value(value);
        }
        numeric_value(value)
    }

    pub(super) fn interpret_date(
        &self,
        value: CellValue,
        kind: Option<crabxl_core::DateKind>,
    ) -> Result<CellValue> {
        use crabxl_core::{DateKind, DateReadPolicy, ExcelDateTime};
        let Some(mut kind) = kind else {
            return Ok(value);
        };
        let serial = match &value {
            CellValue::Integer(v) => *v as f64,
            CellValue::Number(v) => *v,
            CellValue::BigInteger(v) => v
                .as_str()
                .parse::<f64>()
                .map_err(|_| self.invalid("Invalid numeric date serial"))?,
            _ => return Ok(value),
        };
        if !serial.is_finite() {
            return if self.options.date_policy == DateReadPolicy::Compatible {
                Ok(CellValue::error("#VALUE!"))
            } else {
                Err(self.invalid("Non-finite date serial"))
            };
        }
        let raw = ExcelDateTime::from_serial(serial, self.epoch, kind)?;
        if kind == DateKind::DateTime
            && (0.0..1.0).contains(&serial)
            && raw.fraction_milliseconds() < 86_400_000
        {
            kind = DateKind::Time;
        }
        let date = ExcelDateTime::from_serial(serial, self.epoch, kind)?;
        if self.options.date_policy == DateReadPolicy::Compatible {
            let valid = date.is_reference_representable();
            if !valid {
                return Ok(CellValue::error("#VALUE!"));
            }
        }
        Ok(CellValue::DateTime(Box::new(date)))
    }
    pub(super) fn read_inline_text(&mut self) -> Result<CellValue> {
        let preserve = self
            .options
            .inline_rich_text
            .unwrap_or(self.options.rich_text);
        let mut parsed = crate::rich_text::read_container(
            &mut self.xml,
            5,
            b"is",
            self.limits.max_cell_bytes,
            preserve,
        )?;
        if preserve {
            parsed.unprotect();
        }
        Ok(parsed.into_value())
    }

    // Cache-only compatibility ignores expression semantics, but still validates XML.
    pub(super) fn skip_formula_text(&mut self) -> Result<()> {
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Text(text) => {
                    text.xml10_content();
                }
                Event::CData(text) => {
                    text.xml10_content();
                }
                event @ Event::GeneralRef(_) => {
                    self.value_buffer.clear();
                    append_xml_text(&mut self.value_buffer, &event, 4)?;
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 4
                        && e.local_name().as_ref().as_bytes() == b"f" =>
                {
                    return Ok(());
                }
                Event::Comment(_) | Event::PI(_) => {}
                _ => return Err(self.invalid("Invalid formula text content")),
            }
        }
    }

    pub(super) fn read_formula_text(&mut self) -> Result<Box<str>> {
        match self.read_value::<false>(ScalarKind::Text)? {
            CellValue::Text(value) => Ok((*value).into_string()),
            CellValue::Empty => Ok("".into()),
            _ => Err(self.invalid("Invalid formula text")),
        }
    }
    pub(super) fn skip_cell(&mut self, address: CellAddress) -> Result<()> {
        let strict = self.options.formula_policy == crabxl_core::FormulaReadPolicy::ValidateGroups;
        let future_selected_rows = self
            .options
            .rows
            .as_ref()
            .is_none_or(|range| address.row <= *range.end());
        let retain_shared = strict || (!self.options.data_only && future_selected_rows);
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 5
                        && e.local_name().as_ref().as_bytes() == b"f"
                        && retain_shared =>
                {
                    if crate::formula_codec::is_shared(&e)? {
                        let mut metadata = crate::formula_codec::header(
                            &e,
                            self.limits.max_cell_bytes,
                            self.options.formula_policy.into(),
                        )?;
                        let expression = self.read_formula_text()?;
                        let index = match &metadata.kind {
                            crabxl_core::FormulaType::Shared { index, .. } => index,
                            _ => return Err(self.invalid("Invalid projected shared formula")),
                        };
                        if !self.shared_formulas.contains(index)
                            || self.options.formula_policy
                                == crabxl_core::FormulaReadPolicy::ValidateGroups
                        {
                            self.limit_formula_storage(&metadata, &expression)?;
                            self.shared_formulas.resolve(
                                address,
                                expression,
                                &mut metadata,
                                self.options.formula_policy,
                                self.limits.max_cell_bytes,
                            )?;
                        }
                    }
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref().as_bytes() == b"c" =>
                {
                    return Ok(());
                }
                Event::Eof => return Err(self.invalid("Unexpected end of excluded cell")),
                _ => {}
            }
        }
    }
    pub(super) fn finish_xml(&mut self) -> Result<()> {
        let mut in_merges = false;
        let mut seen_merges = false;
        loop {
            let frame = self.xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"sheetData" =>
                {
                    return Err(self.invalid("Duplicate sheetData element"));
                }
                Event::Start(e)
                    if self.capture_merges
                        && frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"mergeCells" =>
                {
                    if seen_merges {
                        return Err(self.invalid("Duplicate mergeCells element"));
                    }
                    seen_merges = true;
                    in_merges = true;
                }
                Event::Empty(e)
                    if self.capture_merges
                        && frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"mergeCells" =>
                {
                    if seen_merges {
                        return Err(self.invalid("Duplicate mergeCells element"));
                    }
                    seen_merges = true;
                }
                Event::End(e)
                    if in_merges
                        && frame.depth == 1
                        && e.local_name().as_ref().as_bytes() == b"mergeCells" =>
                {
                    in_merges = false;
                }
                Event::Start(e) | Event::Empty(e)
                    if in_merges
                        && frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref().as_bytes() == b"mergeCell" =>
                {
                    let mut range = None;
                    for attribute in e.attributes() {
                        let attribute = attribute.map_err(|cause| {
                            Error::caused_by(
                                ErrorKind::Xml,
                                "Invalid merged range attribute",
                                cause,
                            )
                        })?;
                        if attribute.key.as_ref().as_bytes() == b"ref" {
                            range = Some(
                                attribute
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|cause| {
                                        Error::caused_by(
                                            ErrorKind::Xml,
                                            "Invalid merged range reference",
                                            cause,
                                        )
                                    })?
                                    .parse::<crabxl_core::CellRange>()?,
                            );
                        }
                    }
                    let range = range.ok_or_else(|| {
                        Error::new(ErrorKind::InvalidData, "Merged range has no reference")
                    })?;
                    self.retain_merge(range)?;
                }
                Event::Eof => return Ok(()),
                _ => {}
            }
        }
    }
}
