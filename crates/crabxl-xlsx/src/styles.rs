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
    if let Some(name) = &style.font.name {
        crate::encode::validate_xml_text(name)?;
    }
    crate::encode::validate_xml_text(&style.number_format)?;
    Ok(())
}
pub(crate) fn validate_catalog(catalog: &crabxl_core::StyleCatalog) -> Result<()> {
    if !catalog.unmodeled_sections.is_empty()
        || catalog
            .differential_styles
            .iter()
            .any(|style| style.unmodeled_extensions)
        || catalog
            .cell_formats
            .iter()
            .chain(&catalog.base_formats)
            .any(|format| format.unmodeled_extensions)
    {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "Unmodeled style sections require original-package preservation",
        ));
    }
    for font in &catalog.fonts {
        if let Some(name) = &font.name {
            crate::encode::validate_xml_text(name)?;
        }
    }
    for format in &catalog.number_formats {
        crate::encode::validate_xml_text(format.code())?;
    }
    for style in &catalog.named_styles {
        crate::encode::validate_xml_text(&style.name)?;
    }
    for differential in &catalog.differential_styles {
        if let Some(font) = &differential.font {
            if let Some(name) = &font.name {
                crate::encode::validate_xml_text(name)?;
            }
        }
        if let Some(number) = &differential.number_format {
            crate::encode::validate_xml_text(number.code())?;
        }
    }
    if let Some(tables) = &catalog.table_styles {
        for text in [&tables.default_table_style, &tables.default_pivot_style]
            .into_iter()
            .flatten()
        {
            crate::encode::validate_xml_text(text)?;
        }
        for style in &tables.styles {
            crate::encode::validate_xml_text(&style.name)?;
        }
    }
    Ok(())
}
fn write_xf(
    output: &mut impl Write,
    format: &crabxl_core::CellFormat,
    policy: crate::StyleWritePolicy,
) -> io::Result<()> {
    if format.unmodeled_extensions {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Cannot serialize an unmodeled style extension",
        ));
    }
    write!(
        output,
        "<xf numFmtId=\"{}\" fontId=\"{}\" fillId=\"{}\" borderId=\"{}\"",
        format.number_format_id, format.font_id, format.fill_id, format.border_id
    )?;
    if let Some(id) = format.base_format_id {
        write!(output, " xfId=\"{id}\"")?;
    }
    for (name, value) in [
        ("applyNumberFormat", format.apply_number_format),
        ("applyFont", format.apply_font),
        ("applyFill", format.apply_fill),
        ("applyBorder", format.apply_border),
        ("applyAlignment", format.apply_alignment),
        ("applyProtection", format.apply_protection),
        ("quotePrefix", format.quote_prefix),
        ("pivotButton", format.pivot_button),
    ] {
        if let Some(value) = value {
            write!(output, " {name}=\"{}\"", u8::from(value))?;
        }
    }
    output.write_all(b">")?;
    if let Some(alignment) = &format.alignment {
        crate::style_codec::write_alignment(output, alignment, policy)?;
    }
    if let Some(protection) = &format.protection {
        crate::style_codec::write_protection(output, protection)?;
    }
    output.write_all(b"</xf>")
}
pub(crate) fn write_styles(
    output: &mut impl Write,
    catalog: &crabxl_core::StyleCatalog,
    policy: crate::StyleWritePolicy,
) -> io::Result<()> {
    if !catalog.unmodeled_sections.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Cannot serialize unmodeled style sections",
        ));
    }
    write!(
        output,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><styleSheet xmlns=\"{}\"><numFmts count=\"{}\">",
        crate::xml::MAIN_URI,
        catalog.number_formats.len()
    )?;
    for number in &catalog.number_formats {
        write!(output, "<numFmt numFmtId=\"{}\"", number.id())?;
        crate::encode::write_attribute(output, "formatCode", number.code())?;
        output.write_all(b"/>")?;
    }
    write!(
        output,
        "</numFmts><fonts count=\"{}\">",
        catalog.fonts.len()
    )?;
    for font in &catalog.fonts {
        crate::formatting::write_font(output, font, crate::formatting::FontContext::Cell)?;
    }
    write!(output, "</fonts><fills count=\"{}\">", catalog.fills.len())?;
    for fill in &catalog.fills {
        crate::style_codec::write_fill(output, fill)?;
    }
    write!(
        output,
        "</fills><borders count=\"{}\">",
        catalog.borders.len()
    )?;
    for border in &catalog.borders {
        crate::style_codec::write_border(output, border)?;
    }
    write!(
        output,
        "</borders><cellStyleXfs count=\"{}\">",
        catalog.base_formats.len()
    )?;
    for format in &catalog.base_formats {
        write_xf(output, format, policy)?;
    }
    write!(
        output,
        "</cellStyleXfs><cellXfs count=\"{}\">",
        catalog.cell_formats.len()
    )?;
    for format in &catalog.cell_formats {
        write_xf(output, format, policy)?;
    }
    write!(
        output,
        "</cellXfs><cellStyles count=\"{}\">",
        catalog.named_styles.len()
    )?;
    for style in &catalog.named_styles {
        output.write_all(b"<cellStyle")?;
        crate::encode::write_attribute(output, "name", &style.name)?;
        write!(output, " xfId=\"{}\"", style.base_format_id)?;
        for (name, value) in [
            ("builtinId", style.builtin_id),
            ("iLevel", style.outline_level),
        ] {
            if let Some(value) = value {
                write!(output, " {name}=\"{value}\"")?;
            }
        }
        for (name, value) in [
            ("hidden", style.hidden),
            ("customBuiltin", style.custom_builtin),
        ] {
            if let Some(value) = value {
                write!(output, " {name}=\"{}\"", u8::from(value))?;
            }
        }
        output.write_all(b"/>")?;
    }
    output.write_all(b"</cellStyles>")?;
    crate::style_extras_codec::write(output, catalog, policy)?;
    if !catalog.indexed_colors.is_empty() || !catalog.recent_colors.is_empty() {
        output.write_all(b"<colors>")?;
        if !catalog.indexed_colors.is_empty() {
            output.write_all(b"<indexedColors>")?;
            for value in &catalog.indexed_colors {
                write!(output, "<rgbColor rgb=\"{value:08X}\"/>")?;
            }
            output.write_all(b"</indexedColors>")?;
        }
        if !catalog.recent_colors.is_empty() {
            output.write_all(b"<mruColors>")?;
            for value in &catalog.recent_colors {
                crate::formatting::write_color(output, "color", *value)?;
            }
            output.write_all(b"</mruColors>")?;
        }
        output.write_all(b"</colors>")?;
    }
    output.write_all(b"</styleSheet>")
}

// Date-format scan adapted from calamine's detect_custom_number_format,
// Copyright 2016-2026 Johann Tuffe. Uses bounded bracket depth and preserves
// first-section, quoted literal and escaped-character handling.
