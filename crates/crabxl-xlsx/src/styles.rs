// SPDX-License-Identifier: MIT
// Basic style serialization adapted from rust_xlsxwriter's styles.rs,
// Copyright 2022-2026 John McNamara. See third_party/ports.json.
use crabxl_core::{CellStyle, Error, ErrorKind, Result};
use std::io::{self, Write};

pub(crate) fn validate(style: &CellStyle, maximum: usize) -> Result<()> {
    if style.heap_bytes() > maximum
        || style.font.name.chars().count() > 31
        || style.number_format.chars().count() > 255
    {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            "Style payload exceeds metadata limit",
        ));
    }
    if style.font.name.is_empty()
        || style.number_format.is_empty()
        || !style.font.size.is_finite()
        || !(0.0..=409.0).contains(&style.font.size)
        || style.font.size == 0.0
        || style.rotation > 180
        || (style.wrap_text && style.shrink_to_fit)
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Invalid basic cell format",
        ));
    }
    crate::encode::validate_xml_text(&style.font.name)?;
    crate::encode::validate_xml_text(&style.number_format)?;
    for color in [style.font.color, style.fill]
        .into_iter()
        .chain(style.borders.iter().flatten().map(|side| side.color))
    {
        if color.is_some_and(|value| value > 0xffffff) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Color is not an RGB value",
            ));
        }
    }
    Ok(())
}
pub(crate) fn attr(value: &str) -> String {
    quick_xml::escape::escape(value)
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}
pub(crate) fn write_styles(output: &mut impl Write, styles: &[CellStyle]) -> io::Result<()> {
    write!(
        output,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><styleSheet xmlns=\"{}\">",
        crate::xml::MAIN_URI
    )?;
    write!(output, "<numFmts count=\"{}\">", styles.len() - 1)?;
    for (index, style) in styles.iter().enumerate().skip(1) {
        write!(
            output,
            "<numFmt numFmtId=\"{}\" formatCode=\"{}\"/>",
            index + 163,
            attr(&style.number_format)
        )?;
    }
    write!(output, "</numFmts><fonts count=\"{}\">", styles.len())?;
    for style in styles {
        let font = &style.font;
        write!(
            output,
            "<font><sz val=\"{}\"/><name val=\"{}\"/>",
            font.size,
            attr(&font.name)
        )?;
        if font.bold {
            output.write_all(b"<b/>")?;
        }
        if font.italic {
            output.write_all(b"<i/>")?;
        }
        if font.underline {
            output.write_all(b"<u/>")?;
        }
        if let Some(rgb) = font.color {
            write!(output, "<color rgb=\"FF{rgb:06X}\"/>")?;
        }
        output.write_all(b"</font>")?;
    }
    write!(
        output,
        "</fonts><fills count=\"{}\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>",
        styles.len() + 2
    )?;
    for style in styles {
        if let Some(rgb) = style.fill {
            write!(
                output,
                "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{rgb:06X}\"/><bgColor indexed=\"64\"/></patternFill></fill>"
            )?;
        } else {
            output.write_all(b"<fill><patternFill patternType=\"none\"/></fill>")?;
        }
    }
    write!(output, "</fills><borders count=\"{}\">", styles.len())?;
    for style in styles {
        output.write_all(b"<border>")?;
        for (name, side) in ["left", "right", "top", "bottom"].iter().zip(style.borders) {
            if let Some(side) = side {
                write!(output, "<{name} style=\"{}\">", side.line.as_str())?;
                if let Some(rgb) = side.color {
                    write!(output, "<color rgb=\"FF{rgb:06X}\"/>")?;
                }
                write!(output, "</{name}>")?;
            } else {
                write!(output, "<{name}/>")?;
            }
        }
        output.write_all(b"<diagonal/></border>")?;
    }
    write!(
        output,
        "</borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"{}\">",
        styles.len()
    )?;
    for (index, style) in styles.iter().enumerate() {
        let num_fmt = if index == 0 { 0 } else { index + 163 };
        write!(
            output,
            "<xf numFmtId=\"{num_fmt}\" fontId=\"{index}\" fillId=\"{}\" borderId=\"{index}\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\" applyAlignment=\"1\" applyProtection=\"1\"><alignment horizontal=\"{}\" vertical=\"{}\" wrapText=\"{}\" shrinkToFit=\"{}\" textRotation=\"{}\"/><protection locked=\"{}\" hidden=\"{}\"/></xf>",
            if index == 0 { 0 } else { index + 2 },
            style.horizontal.as_str(),
            style.vertical.as_str(),
            u8::from(style.wrap_text),
            u8::from(style.shrink_to_fit),
            style.rotation,
            u8::from(style.locked),
            u8::from(style.hidden)
        )?;
    }
    output.write_all(b"</cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>")
}

// Date-format scan adapted from calamine's detect_custom_number_format,
// Copyright 2016-2026 Johann Tuffe. Uses bounded bracket depth and preserves
// first-section, quoted literal and escaped-character handling.
pub(crate) fn date_format(format: &str) -> Option<crabxl_core::DateKind> {
    let mut escaped = false;
    let mut quote = false;
    let mut brackets = 0usize;
    let mut previous = ' ';
    let mut elapsed = false;
    let mut am_pm = false;
    for ch in format.chars() {
        match (ch, escaped, quote, am_pm, brackets) {
            (_, true, ..) => escaped = false,
            ('_' | '\\' | '*', ..) => escaped = true,
            ('"', _, true, _, _) => quote = false,
            (_, _, true, _, _) => {}
            ('"', _, _, _, _) => quote = true,
            (';', ..) => return None,
            ('[', ..) => brackets += 1,
            (']', .., 1) if elapsed => return Some(crabxl_core::DateKind::Duration),
            (']', ..) => brackets = brackets.saturating_sub(1),
            ('a' | 'A', _, _, false, 0) => am_pm = true,
            ('p' | 'm' | '/' | 'P' | 'M', _, _, true, 0) => {
                return Some(crabxl_core::DateKind::DateTime);
            }
            ('d' | 'm' | 'h' | 'y' | 's' | 'D' | 'M' | 'H' | 'Y' | 'S', _, _, false, 0) => {
                return Some(crabxl_core::DateKind::DateTime);
            }
            _ => {
                if !(elapsed && ch.eq_ignore_ascii_case(&previous)) {
                    elapsed = previous == '[' && matches!(ch, 'm' | 'h' | 's' | 'M' | 'H' | 'S');
                }
            }
        }
        previous = ch;
    }
    None
}
