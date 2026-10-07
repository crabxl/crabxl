// SPDX-License-Identifier: MIT
// Serialization of CrabXL's canonical sparse dimension records.
use crabxl_core::{ColumnDimension, RowDimension};
use std::io::{self, Write};

fn attribute(
    output: &mut impl Write,
    name: &str,
    value: Option<impl std::fmt::Display>,
) -> io::Result<()> {
    if let Some(value) = value {
        write!(output, " {name}=\"{value}\"")?;
    }
    Ok(())
}
fn flag(output: &mut impl Write, name: &str, value: Option<bool>) -> io::Result<()> {
    attribute(output, name, value.map(u8::from))
}
pub(crate) fn write_row_attributes(output: &mut impl Write, row: &RowDimension) -> io::Result<()> {
    attribute(output, "ht", row.height)?;
    attribute(output, "s", row.style.map(|id| id.get()))?;
    flag(output, "hidden", row.hidden)?;
    attribute(output, "outlineLevel", row.outline_level)?;
    flag(output, "collapsed", row.collapsed)?;
    flag(output, "customHeight", row.custom_height)?;
    flag(output, "customFormat", row.custom_format)?;
    flag(output, "thickTop", row.thick_top)?;
    flag(output, "thickBot", row.thick_bottom)?;
    // x14ac descent requires an extension namespace; preserve it in the model,
    // and reject package creation until extension-aware output is available.
    Ok(())
}
pub(crate) fn write_columns(
    output: &mut impl Write,
    columns: &[ColumnDimension],
    namespace: Option<&str>,
) -> io::Result<()> {
    if columns.is_empty() {
        return Ok(());
    }
    output.write_all(b"<cols")?;
    if let Some(namespace) = namespace {
        crate::encode::write_attribute(output, "xmlns", namespace)?;
    }
    output.write_all(b">")?;
    for column in columns {
        write!(
            output,
            "<col min=\"{}\" max=\"{}\"",
            column.start.get() + 1,
            column.end.get() + 1
        )?;
        attribute(output, "width", column.width)?;
        attribute(output, "style", column.style.map(|id| id.get()))?;
        flag(output, "hidden", column.hidden)?;
        flag(output, "bestFit", column.best_fit)?;
        attribute(output, "outlineLevel", column.outline_level)?;
        flag(output, "collapsed", column.collapsed)?;
        flag(output, "customWidth", column.custom_width)?;
        flag(output, "phonetic", column.phonetic)?;
        output.write_all(b"/>")?;
    }
    output.write_all(b"</cols>")
}

fn invalid(message: &'static str) -> crabxl_core::Error {
    crabxl_core::Error::new(crabxl_core::ErrorKind::InvalidData, message)
}
fn parse<T: std::str::FromStr>(value: &str) -> crabxl_core::Result<T> {
    value
        .parse()
        .map_err(|_| invalid("Invalid dimension attribute"))
}
fn boolean(value: &str) -> crabxl_core::Result<bool> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(invalid("Invalid dimension boolean")),
    }
}
pub(crate) fn read_row(
    start: &quick_xml::events::BytesStart<'_>,
    index: crabxl_core::RowIndex,
) -> crabxl_core::Result<Option<RowDimension>> {
    let mut row = RowDimension::new(index);
    let mut explicit = false;
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|_| invalid("Invalid row dimension attribute"))?;
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|_| invalid("Invalid dimension attribute value"))?;
        match attribute.key.as_ref().as_bytes() {
            b"r" | b"spans" => continue,
            b"ht" => row.height = Some(parse(&value)?),
            b"s" => row.style = Some(crabxl_core::StyleId::new(parse(&value)?)),
            b"hidden" => row.hidden = Some(boolean(&value)?),
            b"outlineLevel" => row.outline_level = Some(parse(&value)?),
            b"collapsed" => row.collapsed = Some(boolean(&value)?),
            b"customHeight" => row.custom_height = Some(boolean(&value)?),
            b"customFormat" => row.custom_format = Some(boolean(&value)?),
            b"thickTop" => row.thick_top = Some(boolean(&value)?),
            b"thickBot" => row.thick_bottom = Some(boolean(&value)?),
            _ => continue,
        }
        explicit = true;
    }
    row.validate()?;
    Ok(explicit.then_some(row))
}
pub(crate) fn read_columns<R: std::io::BufRead>(
    xml: &mut crate::xml::XmlStream<R>,
    maximum: usize,
) -> crabxl_core::Result<crabxl_core::SheetDimensions> {
    use crate::xml::Scope;
    use quick_xml::events::Event;
    let mut dimensions = crabxl_core::SheetDimensions::default();
    let mut in_columns = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == 2
                    && frame.scope == Scope::Spreadsheet
                    && e.local_name().as_ref().as_bytes() == b"sheetData" =>
            {
                return Ok(dimensions);
            }
            Event::Start(e)
                if frame.depth == 2
                    && frame.scope == Scope::Spreadsheet
                    && e.local_name().as_ref().as_bytes() == b"cols" =>
            {
                in_columns = true
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && e.local_name().as_ref().as_bytes() == b"cols" =>
            {
                in_columns = false
            }
            Event::Start(e)
                if in_columns
                    && frame.depth == 3
                    && frame.scope == Scope::Spreadsheet
                    && e.local_name().as_ref().as_bytes() == b"col" =>
            {
                let mut column = ColumnDimension::new(
                    crabxl_core::ColumnIndex::new(0)?,
                    crabxl_core::ColumnIndex::new(0)?,
                )?;
                let mut first = false;
                let mut last = false;
                for attribute in e.attributes() {
                    let attribute =
                        attribute.map_err(|_| invalid("Invalid column dimension attribute"))?;
                    let value = attribute
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| invalid("Invalid dimension attribute value"))?;
                    match attribute.key.as_ref().as_bytes() {
                        b"min" | b"max" => {
                            let index = parse::<u32>(&value)?
                                .checked_sub(1)
                                .ok_or_else(|| invalid("Invalid column dimension index"))?;
                            let index = crabxl_core::ColumnIndex::new(index)?;
                            if attribute.key.as_ref().as_bytes() == b"min" {
                                column.start = index;
                                first = true;
                            } else {
                                column.end = index;
                                last = true;
                            }
                        }
                        b"width" => column.width = Some(parse(&value)?),
                        b"style" => column.style = Some(crabxl_core::StyleId::new(parse(&value)?)),
                        b"hidden" => column.hidden = Some(boolean(&value)?),
                        b"bestFit" => column.best_fit = Some(boolean(&value)?),
                        b"outlineLevel" => column.outline_level = Some(parse(&value)?),
                        b"collapsed" => column.collapsed = Some(boolean(&value)?),
                        b"customWidth" => column.custom_width = Some(boolean(&value)?),
                        b"phonetic" => column.phonetic = Some(boolean(&value)?),
                        _ => continue,
                    }
                }
                if !first || !last {
                    return Err(invalid("Missing column dimension interval"));
                }
                dimensions.set_column(column, maximum)?;
            }
            Event::Eof => return Err(invalid("Worksheet has no sheetData element")),
            _ => {}
        }
    }
}
