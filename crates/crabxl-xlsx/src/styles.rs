// SPDX-License-Identifier: MIT
// Basic style serialization adapted from rust_xlsxwriter's styles.rs,
// Copyright 2022-2026 John McNamara. See third_party/ports.json.
use crabxl_core::{CellStyle, Error, ErrorKind, Result};
use std::io::{self, Write};

pub(crate) fn validate(style: &CellStyle, maximum: usize) -> Result<()> {
    if style.heap_bytes() > maximum {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            "Style payload exceeds metadata limit",
        ));
    }
    if style.number_format.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "Number format is empty"));
    }
    crate::formatting::validate_font(&style.font)?;
    crate::encode::validate_xml_text(&style.number_format)?;
    crate::style_codec::validate_fill(&style.fill)?;
    crate::style_codec::validate_border(&style.borders)?;
    crate::style_codec::validate_alignment(&style.alignment)?;
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
        crate::formatting::write_font(output, &style.font, crate::formatting::FontContext::Cell)?;
    }
    write!(
        output,
        "</fonts><fills count=\"{}\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>",
        styles.len() + 2
    )?;
    for style in styles {
        crate::style_codec::write_fill(output, &style.fill)?;
    }
    write!(output, "</fills><borders count=\"{}\">", styles.len())?;
    for style in styles {
        crate::style_codec::write_border(output, &style.borders)?;
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
            "<xf numFmtId=\"{num_fmt}\" fontId=\"{index}\" fillId=\"{}\" borderId=\"{index}\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\" applyAlignment=\"1\" applyProtection=\"1\">",
            if index == 0 { 0 } else { index + 2 }
        )?;
        crate::style_codec::write_alignment(output, &style.alignment)?;
        crate::style_codec::write_protection(output, &style.protection)?;
        output.write_all(b"</xf>")?;
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
