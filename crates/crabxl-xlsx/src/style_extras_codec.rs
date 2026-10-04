//! Differential/table style codecs using canonical components and shared budgets.
use crate::{
    style_reader::{Budget, blank, boolean, integer},
    xml::{Scope, XmlStream, attribute, required_attribute},
};
use crabxl_core::{
    DifferentialStyle, Error, ErrorKind, NumberFormat, Result, StyleCatalog, TableStyle,
    TableStyleCatalog, TableStyleElement, TableStyleRegion,
};
use quick_xml::{
    encoding::Decoder,
    events::{BytesStart, Event},
};
use std::io::{BufRead, Write};
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn limit() -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "Differential/table style allowance exceeded",
    )
}
fn charge(budget: &mut Budget, bytes: usize) -> Result<()> {
    budget.used = budget
        .used
        .checked_add(bytes)
        .filter(|used| *used <= budget.maximum)
        .ok_or_else(limit)?;
    Ok(())
}
pub(crate) fn read_differentials<B: BufRead>(
    xml: &mut XmlStream<B>,
    catalog: &mut StyleCatalog,
    budget: &mut Budget,
) -> Result<()> {
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref() == b"dxf" =>
            {
                crate::formatting::check_attributes(&e, &[])?;
                let value = read_differential(xml, budget.remaining())?;
                let heap = value.heap_bytes();
                budget.push(&mut catalog.differential_styles, value, heap)?;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref() == b"dxfs" =>
            {
                return Ok(());
            }
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid differential style list")),
        }
    }
}
fn read_differential<B: BufRead>(
    xml: &mut XmlStream<B>,
    maximum: usize,
) -> Result<DifferentialStyle> {
    let mut value = DifferentialStyle::default();
    let mut seen = 0u16;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == 4 => {
                let index = match e.local_name().as_ref() {
                    b"font" => 0,
                    b"numFmt" => 1,
                    b"fill" => 2,
                    b"alignment" => 3,
                    b"border" => 4,
                    b"protection" => 5,
                    b"extLst" => 6,
                    _ => 7,
                };
                if index != 7 && seen & (1 << index) != 0 {
                    return Err(invalid("Duplicate differential style component"));
                }
                seen |= 1 << index;
                let remaining = maximum.saturating_sub(value.heap_bytes());
                match index {
                    0 => {
                        crate::formatting::check_attributes(&e, &[])?;
                        if remaining < size_of::<crabxl_core::Font>() {
                            return Err(limit());
                        }
                        let font = crate::formatting::read_font(
                            xml,
                            4,
                            remaining - size_of::<crabxl_core::Font>(),
                            crate::formatting::FontContext::Cell,
                        )?;
                        value.font = Some(Box::new(font));
                    }
                    1 => {
                        crate::formatting::check_attributes(&e, &[b"numFmtId", b"formatCode"])?;
                        let id = integer(&e, b"numFmtId", frame.decoder)?.ok_or_else(|| {
                            invalid("Differential number format identity missing")
                        })?;
                        let code = required_attribute(&e, b"formatCode", frame.decoder)?;
                        crate::encode::validate_xml_text(&code)?;
                        if code.len() > remaining {
                            return Err(limit());
                        }
                        crate::formatting::consume_property(xml, 4)?;
                        value.number_format = Some(NumberFormat::new(id, code));
                    }
                    2 => {
                        crate::formatting::check_attributes(&e, &[])?;
                        if remaining < size_of::<crabxl_core::Fill>() {
                            return Err(limit());
                        }
                        let fill = crate::style_codec::read_fill(
                            xml,
                            4,
                            remaining - size_of::<crabxl_core::Fill>(),
                        )?;
                        value.fill = Some(Box::new(fill));
                    }
                    3 => {
                        if remaining < size_of::<crabxl_core::Alignment>() {
                            return Err(limit());
                        }
                        let alignment = crate::style_codec::read_alignment(&e, frame.decoder)?;
                        crate::formatting::consume_property(xml, 4)?;
                        value.alignment = Some(Box::new(alignment));
                    }
                    4 => {
                        if remaining < size_of::<crabxl_core::Border>() {
                            return Err(limit());
                        }
                        let header = crate::style_codec::read_border_header(&e, frame.decoder)?;
                        value.border =
                            Some(Box::new(crate::style_codec::read_border(xml, 4, header)?));
                    }
                    5 => {
                        value.protection =
                            Some(crate::style_codec::read_protection(&e, frame.decoder)?);
                        crate::formatting::consume_property(xml, 4)?;
                    }
                    _ => {
                        value.unmodeled_extensions = true;
                        crate::style_codec::skip(xml, 4)?;
                    }
                }
                if value.heap_bytes() > maximum {
                    return Err(limit());
                }
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"dxf" =>
            {
                return Ok(value);
            }
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid differential style component content")),
        }
    }
}
pub(crate) fn table_header(header: &BytesStart<'_>, decoder: Decoder) -> Result<TableStyleCatalog> {
    crate::formatting::check_attributes(
        header,
        &[b"count", b"defaultTableStyle", b"defaultPivotStyle"],
    )?;
    attribute(header, b"count", decoder)?;
    let value = TableStyleCatalog {
        default_table_style: attribute(header, b"defaultTableStyle", decoder)?
            .map(String::into_boxed_str),
        default_pivot_style: attribute(header, b"defaultPivotStyle", decoder)?
            .map(String::into_boxed_str),
        ..Default::default()
    };
    for name in [&value.default_table_style, &value.default_pivot_style]
        .into_iter()
        .flatten()
    {
        crate::encode::validate_xml_text(name)?;
    }
    Ok(value)
}
pub(crate) fn read_tables<B: BufRead>(
    xml: &mut XmlStream<B>,
    catalog: &mut StyleCatalog,
    budget: &mut Budget,
    mut value: TableStyleCatalog,
) -> Result<()> {
    charge(budget, size_of::<TableStyleCatalog>() + value.heap_bytes())?;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref() == b"tableStyle" =>
            {
                crate::formatting::check_attributes(&e, &[b"name", b"pivot", b"table", b"count"])?;
                let mut style = TableStyle {
                    name: required_attribute(&e, b"name", frame.decoder)?.into_boxed_str(),
                    pivot: boolean(&e, b"pivot", frame.decoder)?,
                    table: boolean(&e, b"table", frame.decoder)?,
                    count: integer(&e, b"count", frame.decoder)?,
                    elements: Vec::new(),
                };
                crate::encode::validate_xml_text(&style.name)?;
                let mut local = Budget {
                    used: style.name.len(),
                    maximum: budget.remaining(),
                    records: budget.records,
                };
                if local.used > local.maximum {
                    return Err(limit());
                }
                loop {
                    let child = xml.next()?;
                    match child.event {
                        Event::Start(e)
                            if child.scope == Scope::Spreadsheet
                                && child.depth == 4
                                && e.local_name().as_ref() == b"tableStyleElement" =>
                        {
                            crate::formatting::check_attributes(&e, &[b"type", b"size", b"dxfId"])?;
                            let element = TableStyleElement {
                                region: TableStyleRegion::parse(&required_attribute(
                                    &e,
                                    b"type",
                                    child.decoder,
                                )?)?,
                                size: integer(&e, b"size", child.decoder)?,
                                differential_style_id: integer(&e, b"dxfId", child.decoder)?,
                            };
                            crate::formatting::consume_property(xml, 4)?;
                            local.push(&mut style.elements, element, 0)?;
                        }
                        Event::End(e)
                            if child.scope == Scope::Spreadsheet
                                && child.depth == 2
                                && e.local_name().as_ref() == b"tableStyle" =>
                        {
                            break;
                        }
                        ref event if blank(event) => {}
                        _ => return Err(invalid("Invalid table style element")),
                    }
                }
                let heap = style.heap_bytes();
                budget.push(&mut value.styles, style, heap)?;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref() == b"tableStyles" =>
            {
                catalog.table_styles = Some(Box::new(value));
                return Ok(());
            }
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid table style catalog")),
        }
    }
}
pub(crate) fn write(
    output: &mut impl Write,
    catalog: &StyleCatalog,
    policy: crate::StyleWritePolicy,
) -> std::io::Result<()> {
    if !catalog.differential_styles.is_empty() {
        write!(
            output,
            "<dxfs count=\"{}\">",
            catalog.differential_styles.len()
        )?;
        for value in &catalog.differential_styles {
            if value.unmodeled_extensions {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Unmodeled differential extensions",
                ));
            }
            output.write_all(b"<dxf>")?;
            if let Some(font) = &value.font {
                crate::formatting::write_font(output, font, crate::formatting::FontContext::Cell)?;
            }
            if let Some(number) = &value.number_format {
                write!(output, "<numFmt numFmtId=\"{}\"", number.id())?;
                crate::encode::write_attribute(output, "formatCode", number.code())?;
                output.write_all(b"/>")?;
            }
            if let Some(fill) = &value.fill {
                crate::style_codec::write_fill(output, fill)?;
            }
            if let Some(alignment) = &value.alignment {
                crate::style_codec::write_alignment(output, alignment, policy)?;
            }
            if let Some(border) = &value.border {
                crate::style_codec::write_border(output, border)?;
            }
            if let Some(protection) = &value.protection {
                crate::style_codec::write_protection(output, protection)?;
            }
            output.write_all(b"</dxf>")?;
        }
        output.write_all(b"</dxfs>")?;
    }
    if let Some(value) = &catalog.table_styles {
        write!(output, "<tableStyles count=\"{}\"", value.styles.len())?;
        for (name, text) in [
            ("defaultTableStyle", &value.default_table_style),
            ("defaultPivotStyle", &value.default_pivot_style),
        ] {
            if let Some(text) = text {
                crate::encode::write_attribute(output, name, text)?;
            }
        }
        output.write_all(b">")?;
        for style in &value.styles {
            output.write_all(b"<tableStyle")?;
            crate::encode::write_attribute(output, "name", &style.name)?;
            if let Some(count) = style.count {
                write!(output, " count=\"{count}\"")?;
            }
            for (name, flag) in [("pivot", style.pivot), ("table", style.table)] {
                if let Some(flag) = flag {
                    write!(output, " {name}=\"{}\"", u8::from(flag))?;
                }
            }
            output.write_all(b">")?;
            for element in &style.elements {
                write!(
                    output,
                    "<tableStyleElement type=\"{}\"",
                    element.region.as_str()
                )?;
                if let Some(size) = element.size {
                    write!(output, " size=\"{size}\"")?;
                }
                if let Some(id) = element.differential_style_id {
                    write!(output, " dxfId=\"{id}\"")?;
                }
                output.write_all(b"/>")?;
            }
            output.write_all(b"</tableStyle>")?;
        }
        output.write_all(b"</tableStyles>")?;
    }
    Ok(())
}
