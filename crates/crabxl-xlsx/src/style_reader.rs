// SPDX-License-Identifier: MIT
// Selected style/table event flow adapted from umya-spreadsheet and calamine.
// Copyright (c) 2020 MathNya; Copyright 2016-2026 Johann Tuffe.
// Rewritten with relationship-resolved input, stable IDs and byte/count budgets.
use crate::xml::{Scope, XmlStream, attribute, required_attribute};
use crabxl_core::{
    CellFormat, ColorKind, Error, ErrorKind, NamedStyle, NumberFormat, ResourceLimits, Result,
    StyleCatalog,
};
use quick_xml::events::{BytesStart, Event};
use std::io::BufRead;
pub(super) struct Budget {
    pub(super) used: usize,
    pub(super) maximum: usize,
    pub(super) records: usize,
}
impl Budget {
    pub(super) fn remaining(&self) -> usize {
        self.maximum.saturating_sub(self.used)
    }
    pub(super) fn push<T>(&mut self, target: &mut Vec<T>, value: T, heap: usize) -> Result<()> {
        if target.len() >= self.records || heap > self.remaining() {
            return Err(limit("Style record/payload limit exceeded"));
        }
        if target.len() == target.capacity() {
            let allowed = self.remaining().saturating_sub(heap) / size_of::<T>();
            if allowed == 0 {
                return Err(limit("Style slot limit exceeded"));
            }
            let additional = target
                .capacity()
                .max(4)
                .min(allowed)
                .min(self.records - target.len());
            let before = target.capacity();
            target.try_reserve_exact(additional).map_err(|e| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot allocate style table",
                    e,
                )
            })?;
            self.used = self
                .used
                .checked_add((target.capacity() - before) * size_of::<T>())
                .ok_or_else(|| limit("Style capacity overflows"))?;
        }
        self.used = self
            .used
            .checked_add(heap)
            .filter(|n| *n <= self.maximum)
            .ok_or_else(|| limit("Retained style catalog exceeds byte limit"))?;
        target.push(value);
        Ok(())
    }
}
fn limit(s: &str) -> Error {
    Error::new(ErrorKind::LimitExceeded, s)
}
fn invalid(s: &str) -> Error {
    Error::new(ErrorKind::InvalidData, s)
}
pub(super) fn blank(e: &Event<'_>) -> bool {
    matches!(e,Event::Text(t) if t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace))
        || matches!(e, Event::Comment(_) | Event::PI(_))
}
pub(super) fn integer(e: &BytesStart<'_>, key: &[u8]) -> Result<Option<u32>> {
    attribute(e, key)?
        .map(|n| {
            n.trim()
                .parse()
                .map_err(|_| invalid("Invalid style identity"))
        })
        .transpose()
}
pub(super) fn boolean(e: &BytesStart<'_>, key: &[u8]) -> Result<Option<bool>> {
    attribute(e, key)?
        .map(|n| crate::formatting::boolean(Some(&n)))
        .transpose()
}
fn format_header(e: &BytesStart<'_>) -> Result<CellFormat> {
    crate::formatting::check_attributes(
        e,
        &[
            b"numFmtId",
            b"fontId",
            b"fillId",
            b"borderId",
            b"xfId",
            b"applyNumberFormat",
            b"applyFont",
            b"applyFill",
            b"applyBorder",
            b"applyAlignment",
            b"applyProtection",
            b"quotePrefix",
            b"pivotButton",
        ],
    )?;
    Ok(CellFormat {
        number_format_id: integer(e, b"numFmtId")?.unwrap_or(0),
        font_id: integer(e, b"fontId")?.unwrap_or(0),
        fill_id: integer(e, b"fillId")?.unwrap_or(0),
        border_id: integer(e, b"borderId")?.unwrap_or(0),
        base_format_id: integer(e, b"xfId")?,
        apply_number_format: boolean(e, b"applyNumberFormat")?,
        apply_font: boolean(e, b"applyFont")?,
        apply_fill: boolean(e, b"applyFill")?,
        apply_border: boolean(e, b"applyBorder")?,
        apply_alignment: boolean(e, b"applyAlignment")?,
        apply_protection: boolean(e, b"applyProtection")?,
        quote_prefix: boolean(e, b"quotePrefix")?,
        pivot_button: boolean(e, b"pivotButton")?,
        ..Default::default()
    })
}
fn read_format<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    mut value: CellFormat,
) -> Result<CellFormat> {
    loop {
        let f = xml.next()?;
        match f.event {
            Event::Start(e) if f.scope == Scope::Spreadsheet && f.depth == depth + 1 => {
                match e.local_name().as_ref().as_bytes() {
                    b"alignment" => {
                        if value.alignment.is_some() {
                            return Err(invalid("Duplicate format alignment"));
                        }
                        let alignment = crate::style_codec::read_alignment(&e)?;
                        crate::formatting::consume_property(xml, depth + 1)?;
                        value.alignment = Some(Box::new(alignment));
                    }
                    b"protection" => {
                        if value.protection.is_some() {
                            return Err(invalid("Duplicate format protection"));
                        }
                        let protection = crate::style_codec::read_protection(&e)?;
                        crate::formatting::consume_property(xml, depth + 1)?;
                        value.protection = Some(protection);
                    }
                    _ => {
                        value.unmodeled_extensions = true;
                        crate::style_codec::skip(xml, depth + 1)?;
                    }
                }
            }
            Event::End(e)
                if f.scope == Scope::Spreadsheet
                    && f.depth + 1 == depth
                    && e.local_name().as_ref().as_bytes() == b"xf" =>
            {
                return Ok(value);
            }
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid format record content")),
        }
    }
}
pub(crate) fn read<B: BufRead>(
    input: B,
    part: String,
    limits: ResourceLimits,
    maximum: usize,
    records: usize,
) -> Result<StyleCatalog> {
    read_impl(input, part.clone(), limits, maximum, records).map_err(|e| e.with_part(part))
}
fn read_impl<B: BufRead>(
    input: B,
    part: String,
    limits: ResourceLimits,
    maximum: usize,
    records: usize,
) -> Result<StyleCatalog> {
    if maximum < size_of::<StyleCatalog>() || records == 0 {
        return Err(limit("Style catalog allowance is too small"));
    }
    let mut xml = XmlStream::new(input, part, limits.max_part_bytes, limits);
    let mut result = StyleCatalog::default();
    let mut budget = Budget {
        used: size_of::<StyleCatalog>(),
        maximum,
        records,
    };
    let mut root = false;
    let mut seen = 0u32;
    loop {
        let f = xml.next()?;
        match f.event {
            Event::Decl(_) if !root => {}
            Event::Start(e)
                if f.scope == Scope::Spreadsheet
                    && f.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"styleSheet"
                    && !root =>
            {
                root = true;
            }
            Event::Start(e) if f.scope == Scope::Spreadsheet && f.depth == 2 && root => {
                let section = e.local_name().as_ref().as_bytes().to_vec();
                let number = match section.as_slice() {
                    b"numFmts" => 0,
                    b"fonts" => 1,
                    b"fills" => 2,
                    b"borders" => 3,
                    b"cellStyleXfs" => 4,
                    b"cellXfs" => 5,
                    b"cellStyles" => 6,
                    b"colors" => 7,
                    b"dxfs" => 8,
                    b"tableStyles" => 9,
                    _ => 10,
                };
                if number != 10 {
                    if seen & (1 << number) != 0 {
                        return Err(invalid("Duplicate style section"));
                    }
                    seen |= 1 << number;
                }
                if number == 10 {
                    let name = String::from_utf8(section)
                        .map_err(|_| invalid("Invalid style section name"))?
                        .into_boxed_str();
                    let heap = name.len();
                    budget.push(&mut result.unmodeled_sections, name, heap)?;
                    crate::style_codec::skip(&mut xml, 2)?;
                    continue;
                }
                if number == 8 {
                    crate::formatting::check_attributes(&e, &[b"count"])?;
                    attribute(&e, b"count")?;
                    crate::style_extras_codec::read_differentials(
                        &mut xml,
                        &mut result,
                        &mut budget,
                    )?;
                    continue;
                }
                if number == 9 {
                    let header = crate::style_extras_codec::table_header(&e)?;
                    crate::style_extras_codec::read_tables(
                        &mut xml,
                        &mut result,
                        &mut budget,
                        header,
                    )?;
                    continue;
                }
                if number == 7 {
                    read_colors(&mut xml, &mut result, &mut budget)?;
                    continue;
                }
                loop {
                    let item = xml.next()?;
                    match item.event {
                        Event::Start(e) if item.scope == Scope::Spreadsheet && item.depth == 3 => {
                            match (number, e.local_name().as_ref().as_bytes()) {
                                (0, b"numFmt") => {
                                    crate::formatting::check_attributes(
                                        &e,
                                        &[b"numFmtId", b"formatCode"],
                                    )?;
                                    let id = integer(&e, b"numFmtId")?
                                        .ok_or_else(|| invalid("Number format ID missing"))?;
                                    let code =
                                        required_attribute(&e, b"formatCode")?.into_boxed_str();
                                    crate::encode::validate_xml_text(&code)?;
                                    crate::formatting::consume_property(&mut xml, 3)?;
                                    let heap = code.len();
                                    budget.push(
                                        &mut result.number_formats,
                                        NumberFormat::new(id, code),
                                        heap,
                                    )?;
                                }
                                (1, b"font") => {
                                    crate::formatting::check_attributes(&e, &[])?;
                                    let font = crate::formatting::read_font(
                                        &mut xml,
                                        3,
                                        budget.remaining(),
                                        crate::formatting::FontContext::Cell,
                                    )?;
                                    let heap = font.heap_bytes();
                                    budget.push(&mut result.fonts, font, heap)?;
                                }
                                (2, b"fill") => {
                                    crate::formatting::check_attributes(&e, &[])?;
                                    let fill = crate::style_codec::read_fill(
                                        &mut xml,
                                        3,
                                        budget.remaining(),
                                    )?;
                                    let heap = fill.heap_bytes();
                                    budget.push(&mut result.fills, fill, heap)?;
                                }
                                (3, b"border") => {
                                    let header = crate::style_codec::read_border_header(&e)?;
                                    let border = crate::style_codec::read_border(
                                        &mut xml,
                                        3,
                                        header,
                                        budget.remaining(),
                                    )?;
                                    let heap = border.heap_bytes();
                                    budget.push(&mut result.borders, border, heap)?;
                                }
                                (4 | 5, b"xf") => {
                                    let header = format_header(&e)?;
                                    let format = read_format(&mut xml, 3, header)?;
                                    let heap = format.heap_bytes();
                                    if number == 4 {
                                        budget.push(&mut result.base_formats, format, heap)?;
                                    } else {
                                        budget.push(&mut result.cell_formats, format, heap)?;
                                    }
                                }
                                (6, b"cellStyle") => {
                                    crate::formatting::check_attributes(
                                        &e,
                                        &[
                                            b"name",
                                            b"xfId",
                                            b"builtinId",
                                            b"customBuiltin",
                                            b"hidden",
                                            b"iLevel",
                                        ],
                                    )?;
                                    let style = NamedStyle {
                                        name: required_attribute(&e, b"name")?.into_boxed_str(),
                                        base_format_id: integer(&e, b"xfId")?
                                            .ok_or_else(|| invalid("Named style base missing"))?,
                                        builtin_id: integer(&e, b"builtinId")?,
                                        custom_builtin: boolean(&e, b"customBuiltin")?,
                                        hidden: boolean(&e, b"hidden")?,
                                        outline_level: integer(&e, b"iLevel")?,
                                    };
                                    crate::encode::validate_xml_text(&style.name)?;
                                    crate::formatting::consume_property(&mut xml, 3)?;
                                    let heap = style.name.len();
                                    budget.push(&mut result.named_styles, style, heap)?;
                                }
                                _ => {
                                    return Err(Error::new(
                                        ErrorKind::Unsupported,
                                        "Unknown style table entry",
                                    ));
                                }
                            }
                        }
                        Event::End(e)
                            if item.scope == Scope::Spreadsheet
                                && item.depth == 1
                                && e.local_name().as_ref().as_bytes() == section =>
                        {
                            break;
                        }
                        ref event if blank(event) => {}
                        _ => return Err(invalid("Invalid style table content")),
                    }
                }
            }
            Event::End(e)
                if f.scope == Scope::Spreadsheet
                    && f.depth == 0
                    && e.local_name().as_ref().as_bytes() == b"styleSheet" => {}
            Event::Eof if root => break,
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid style catalog document")),
        }
    }
    result.number_formats.sort_unstable_by_key(|n| n.id());
    if result
        .number_formats
        .windows(2)
        .any(|pair| pair[0].id() == pair[1].id())
    {
        return Err(invalid("Duplicate number-format identity"));
    }
    result.validate_references()?;
    if result.memory_bytes() > maximum {
        return Err(limit("Style catalog retained capacity exceeds limit"));
    }
    Ok(result)
}
fn read_colors<B: BufRead>(
    xml: &mut XmlStream<B>,
    result: &mut StyleCatalog,
    budget: &mut Budget,
) -> Result<()> {
    let mut seen = 0u8;
    loop {
        let f = xml.next()?;
        match f.event {
            Event::Start(e) if f.scope == Scope::Spreadsheet && f.depth == 3 => {
                let indexed = match e.local_name().as_ref().as_bytes() {
                    b"indexedColors" => true,
                    b"mruColors" => false,
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Unknown color palette section",
                        ));
                    }
                };
                let bit = if indexed { 1 } else { 2 };
                if seen & bit != 0 {
                    return Err(invalid("Duplicate color palette section"));
                }
                seen |= bit;
                loop {
                    let item = xml.next()?;
                    match item.event {
                        Event::Start(e) if item.scope == Scope::Spreadsheet && item.depth == 4 => {
                            if (indexed && e.local_name().as_ref().as_bytes() != b"rgbColor")
                                || (!indexed && e.local_name().as_ref().as_bytes() != b"color")
                            {
                                return Err(invalid("Invalid palette entry"));
                            }
                            let color = crate::style_codec::read_color(&e)?;
                            crate::formatting::consume_property(xml, 4)?;
                            if indexed {
                                let rgb = match color.kind {
                                    ColorKind::Argb(value) => {
                                        crabxl_core::ArgbLiteral::from_channels(value)
                                    }
                                    ColorKind::ArgbLiteral(value) => value,
                                    _ => return Err(invalid("Indexed palette entry is not ARGB")),
                                };
                                if color.tint.is_some() {
                                    return Err(invalid("Indexed palette entry has tint"));
                                }
                                budget.push(&mut result.indexed_colors, rgb, 0)?;
                            } else {
                                let heap = color.heap_bytes();
                                budget.push(&mut result.recent_colors, color, heap)?;
                            }
                        }
                        Event::End(e)
                            if item.scope == Scope::Spreadsheet
                                && item.depth == 2
                                && e.local_name().as_ref().as_bytes()
                                    == if indexed {
                                        b"indexedColors".as_slice()
                                    } else {
                                        b"mruColors".as_slice()
                                    } =>
                        {
                            break;
                        }
                        ref event if blank(event) => {}
                        _ => return Err(invalid("Invalid color palette content")),
                    }
                }
            }
            Event::End(e)
                if f.scope == Scope::Spreadsheet
                    && f.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"colors" =>
            {
                return Ok(());
            }
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid color palette document")),
        }
    }
}

pub(crate) struct ImportedStyles {
    pub(crate) catalog: StyleCatalog,
    pub(crate) date_kinds: Vec<Option<crabxl_core::DateKind>>,
}
#[derive(Clone, Copy)]
pub(crate) struct StyleRead<'a> {
    pub(crate) catalog: &'a StyleCatalog,
    pub(crate) imported: &'a ImportedStyles,
}
impl StyleRead<'_> {
    pub(crate) fn kind(&self, id: crabxl_core::StyleId) -> Result<Option<crabxl_core::DateKind>> {
        self.imported.kind(id)
    }
}
impl ImportedStyles {
    pub(crate) fn new(catalog: StyleCatalog, maximum: usize) -> Result<Self> {
        let count = catalog.cell_formats.len();
        if catalog
            .memory_bytes()
            .saturating_add(count * size_of::<Option<crabxl_core::DateKind>>())
            > maximum
        {
            return Err(limit("Style classifications exceed catalog allowance"));
        }
        let mut date_kinds = Vec::new();
        date_kinds.try_reserve_exact(count).map_err(|e| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate style classifications",
                e,
            )
        })?;
        for format in &catalog.cell_formats {
            let kind = catalog
                .number_formats
                .binary_search_by_key(&format.number_format_id, NumberFormat::id)
                .ok()
                .map(|i| catalog.number_formats[i].date_kind())
                .unwrap_or_else(|| match format.number_format_id {
                    14..=22 | 45 | 47 => Some(crabxl_core::DateKind::DateTime),
                    46 => Some(crabxl_core::DateKind::Duration),
                    _ => None,
                });
            date_kinds.push(kind);
        }
        let value = Self {
            catalog,
            date_kinds,
        };
        if value.memory_bytes() > maximum {
            return Err(limit("Style classification capacity exceeds allowance"));
        }
        Ok(value)
    }
    pub(crate) fn memory_bytes(&self) -> usize {
        self.catalog.memory_bytes()
            + self.date_kinds.capacity() * size_of::<Option<crabxl_core::DateKind>>()
    }
    pub(crate) fn kind(&self, id: crabxl_core::StyleId) -> Result<Option<crabxl_core::DateKind>> {
        if self.date_kinds.is_empty() && id.get() == 0 {
            return Ok(None);
        }
        self.date_kinds
            .get(id.get() as usize)
            .copied()
            .ok_or_else(|| invalid("Cell references a missing format"))
    }
}
