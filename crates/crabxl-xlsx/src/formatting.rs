// SPDX-License-Identifier: MIT
// Font/color layouts adapted from umya-spreadsheet, Copyright (c) 2020 MathNya.
// Shared fallible codecs for workbook fonts and rich-run overrides.
use crate::xml::{Scope, XmlStream, attribute};
use crabxl_core::{
    Color, ColorKind, Error, ErrorKind, Font, FontScheme, Result, TextVerticalAlignment, Underline,
};
use quick_xml::{
    encoding::Decoder,
    events::{BytesStart, Event},
};
use std::io::{self, BufRead, Write};
#[derive(Clone, Copy)]
pub(crate) enum FontContext {
    Cell,
    Run,
}
impl FontContext {
    fn container(self) -> &'static str {
        match self {
            Self::Cell => "font",
            Self::Run => "rPr",
        }
    }
    fn name(self) -> &'static [u8] {
        match self {
            Self::Cell => b"name",
            Self::Run => b"rFont",
        }
    }
}
pub(crate) fn read_font<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    maximum: usize,
    context: FontContext,
) -> Result<Font> {
    let mut font = Font::default();
    let mut seen = 0u32;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == depth + 1 => {
                let key = e.local_name();
                let key = key.as_ref();
                let number = match key {
                    key if key == context.name() => 0,
                    b"sz" => 1,
                    b"b" => 2,
                    b"i" => 3,
                    b"strike" => 4,
                    b"outline" => 5,
                    b"shadow" => 6,
                    b"condense" => 7,
                    b"extend" => 8,
                    b"u" => 9,
                    b"vertAlign" => 10,
                    b"charset" => 11,
                    b"family" => 12,
                    b"scheme" => 13,
                    b"color" => 14,
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Unknown run font property",
                        ));
                    }
                };
                if seen & (1 << number) != 0 {
                    return Err(invalid("Duplicate run font property"));
                }
                seen |= 1 << number;
                check_attributes(
                    &e,
                    if key == b"color" {
                        &[b"rgb", b"theme", b"indexed", b"auto", b"tint"]
                    } else {
                        &[b"val"]
                    },
                )?;
                let val = attribute(&e, b"val", frame.decoder)?;
                match key {
                    key if key == context.name() => {
                        font.name = Some(
                            val.ok_or_else(|| invalid("Run font name is missing"))?
                                .into_boxed_str(),
                        )
                    }
                    b"sz" => {
                        font.size = Some(
                            val.ok_or_else(|| invalid("Run font size is missing"))?
                                .parse()
                                .map_err(|_| invalid("Invalid font size"))?,
                        )
                    }
                    b"b" => font.bold = Some(boolean(val.as_deref())?),
                    b"i" => font.italic = Some(boolean(val.as_deref())?),
                    b"strike" => font.strike = Some(boolean(val.as_deref())?),
                    b"outline" => font.outline = Some(boolean(val.as_deref())?),
                    b"shadow" => font.shadow = Some(boolean(val.as_deref())?),
                    b"condense" => font.condense = Some(boolean(val.as_deref())?),
                    b"extend" => font.extend = Some(boolean(val.as_deref())?),
                    b"u" => {
                        font.underline = Some(match val.as_deref().unwrap_or("single") {
                            "none" => Underline::None,
                            "single" => Underline::Single,
                            "double" => Underline::Double,
                            "singleAccounting" => Underline::SingleAccounting,
                            "doubleAccounting" => Underline::DoubleAccounting,
                            _ => return Err(invalid("Invalid underline")),
                        })
                    }
                    b"vertAlign" => {
                        font.vertical = Some(match val.as_deref() {
                            Some("baseline") => TextVerticalAlignment::Baseline,
                            Some("superscript") => TextVerticalAlignment::Superscript,
                            Some("subscript") => TextVerticalAlignment::Subscript,
                            _ => return Err(invalid("Invalid vertical text alignment")),
                        })
                    }
                    b"charset" => {
                        font.charset = Some(
                            val.ok_or_else(|| invalid("Charset is missing"))?
                                .parse()
                                .map_err(|_| invalid("Invalid charset"))?,
                        )
                    }
                    b"family" => {
                        font.family = Some(
                            val.ok_or_else(|| invalid("Font family is missing"))?
                                .parse()
                                .map_err(|_| invalid("Invalid font family"))?,
                        )
                    }
                    b"scheme" => {
                        font.scheme = Some(match val.as_deref() {
                            Some("none") => FontScheme::None,
                            Some("major") => FontScheme::Major,
                            Some("minor") => FontScheme::Minor,
                            _ => return Err(invalid("Invalid font scheme")),
                        })
                    }
                    b"color" => font.color = Some(read_color(&e, frame.decoder)?),
                    _ => {}
                }
                if size_of::<Font>() + font.name.as_ref().map_or(0, |s| s.len()) > maximum {
                    return Err(limit());
                }
                consume_property(xml, depth + 1)?;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref() == context.container().as_bytes() =>
            {
                validate_font(&font)?;
                return Ok(font);
            }
            Event::Start(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown rich-text namespace or subtree",
                ));
            }
            Event::Text(t) if t.iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid run font content")),
        }
    }
}
pub(crate) fn consume_property<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<()> {
    loop {
        let f = xml.next()?;
        match f.event {
            Event::End(_) if f.depth + 1 == depth => return Ok(()),
            Event::Start(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown rich-text namespace or subtree",
                ));
            }
            Event::Text(t) if t.iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Font or phonetic property must be empty")),
        }
    }
}
pub(crate) fn boolean(value: Option<&str>) -> Result<bool> {
    match value {
        None | Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        _ => Err(invalid("Invalid rich-text boolean")),
    }
}
pub(crate) fn read_color(e: &BytesStart<'_>, decoder: Decoder) -> Result<Color> {
    check_attributes(e, &[b"rgb", b"theme", b"indexed", b"auto", b"tint"])?;
    // Decode all known attributes for XML validity, then select public constructor priority.
    // Unselected color identities do not impose their own scalar/hex validation.
    let indexed = attribute(e, b"indexed", decoder)?;
    let theme = attribute(e, b"theme", decoder)?;
    let automatic = attribute(e, b"auto", decoder)?;
    let rgb = attribute(e, b"rgb", decoder)?;
    let kind = if let Some(value) = indexed {
        ColorKind::Indexed(
            value
                .trim()
                .parse()
                .map_err(|_| invalid("Invalid indexed color"))?,
        )
    } else if let Some(value) = theme {
        ColorKind::Theme(
            value
                .trim()
                .parse()
                .map_err(|_| invalid("Invalid theme color"))?,
        )
    } else if let Some(value) = automatic {
        ColorKind::Auto(boolean(Some(value.trim()))?)
    } else if let Some(value) = rgb {
        crabxl_core::ArgbLiteral::parse(&value)?.into_kind()
    } else {
        ColorKind::Unspecified
    };
    let tint = attribute(e, b"tint", decoder)?
        .map(|s| s.parse().map_err(|_| invalid("Invalid color tint")))
        .transpose()?;
    let color = Color { kind, tint };
    validate_color(color)?;
    Ok(color)
}
pub(crate) fn validate_color(value: Color) -> Result<()> {
    value.validate()
}
pub(crate) fn validate_font(value: &Font) -> Result<()> {
    if let Some(name) = &value.name {
        crate::encode::validate_xml_text(name)?;
    }
    value.validate()
}
pub(crate) fn write_font(
    output: &mut impl Write,
    font: &Font,
    context: FontContext,
) -> io::Result<()> {
    write!(output, "<{}>", context.container())?;
    if let Some(name) = &font.name {
        write!(
            output,
            "<{}",
            std::str::from_utf8(context.name()).map_err(io::Error::other)?
        )?;
        crate::encode::write_attribute(output, "val", name)?;
        output.write_all(b"/>")?;
    }
    if let Some(size) = font.size {
        write!(output, "<sz val=\"{size}\"/>")?;
    }
    for (key, val) in [
        ("b", font.bold),
        ("i", font.italic),
        ("strike", font.strike),
        ("outline", font.outline),
        ("shadow", font.shadow),
        ("condense", font.condense),
        ("extend", font.extend),
    ] {
        if let Some(val) = val {
            write!(output, "<{key} val=\"{}\"/>", u8::from(val))?;
        }
    }
    if let Some(u) = font.underline {
        let token = match u {
            Underline::None => "none",
            Underline::Single => "single",
            Underline::Double => "double",
            Underline::SingleAccounting => "singleAccounting",
            Underline::DoubleAccounting => "doubleAccounting",
        };
        write!(output, "<u val=\"{token}\"/>")?;
    }
    if let Some(v) = font.vertical {
        let token = match v {
            TextVerticalAlignment::Baseline => "baseline",
            TextVerticalAlignment::Superscript => "superscript",
            TextVerticalAlignment::Subscript => "subscript",
        };
        write!(output, "<vertAlign val=\"{token}\"/>")?;
    }
    if let Some(charset) = font.charset {
        write!(output, "<charset val=\"{charset}\"/>")?;
    }
    if let Some(family) = font.family {
        write!(output, "<family val=\"{family}\"/>")?;
    }
    if let Some(scheme) = font.scheme {
        let token = match scheme {
            FontScheme::None => "none",
            FontScheme::Major => "major",
            FontScheme::Minor => "minor",
        };
        write!(output, "<scheme val=\"{token}\"/>")?;
    }
    if let Some(color) = font.color {
        write_color(output, "color", color)?;
    }
    write!(output, "</{}>", context.container())
}
pub(crate) fn check_attributes(e: &BytesStart<'_>, allowed: &[&[u8]]) -> Result<()> {
    for attr in e.attributes() {
        let attr =
            attr.map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid rich-text attribute", e))?;
        let key = attr.key.as_ref();
        if key == b"xmlns" || key.starts_with(b"xmlns:") {
            continue;
        }
        if !allowed.contains(&key) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Unknown formatting property attribute",
            ));
        }
    }
    Ok(())
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn limit() -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "Font allocation exceeds payload byte limit",
    )
}

pub(crate) fn write_color(output: &mut impl Write, element: &str, color: Color) -> io::Result<()> {
    match color.kind {
        ColorKind::Unspecified => write!(output, "<{element}"),
        ColorKind::Argb(v) => write!(output, "<{element} rgb=\"{v:08X}\""),
        ColorKind::ArgbLiteral(v) => write!(output, "<{element} rgb=\"{v}\""),
        ColorKind::Theme(v) => write!(output, "<{element} theme=\"{v}\""),
        ColorKind::Indexed(v) => write!(output, "<{element} indexed=\"{v}\""),
        ColorKind::Auto(v) => write!(output, "<{element} auto=\"{}\"", u8::from(v)),
    }?;
    if let Some(tint) = color.tint {
        write!(output, " tint=\"{tint}\"")?;
    }
    output.write_all(b"/>")?;
    Ok(())
}
