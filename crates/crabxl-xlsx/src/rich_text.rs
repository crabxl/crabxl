// SPDX-License-Identifier: MIT
// Run/font XML flow adapted from umya-spreadsheet, Copyright (c) 2020 MathNya.
// Rewritten for shared core models, namespace-aware fallible streams and budgets.
// Phonetic handling is implemented here; upstream discards that data.

use crate::xml::{Scope, XmlStream, append_xml_text, attribute, required_attribute};
use crabxl_core::{
    CellValue, Error, ErrorKind, PhoneticProperties, PhoneticRun, Result, RichText, RichTextRun,
    RunFont,
};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

pub(crate) enum ParsedText {
    Plain(Box<str>),
    Rich(Box<RichText>),
}
impl ParsedText {
    pub(crate) fn into_value(self) -> CellValue {
        match self {
            Self::Plain(v) => CellValue::text(v),
            Self::Rich(v) => CellValue::RichText(v),
        }
    }
}

/// The public reference removes protection in rich mode within each display run.
#[inline]
pub(crate) fn unprotect(text: &mut Box<str>) {
    if text.contains("x005F_") {
        *text = text.replace("x005F_", "").into_boxed_str();
    }
}
impl ParsedText {
    pub(crate) fn unprotect(&mut self) {
        match self {
            Self::Plain(text) => unprotect(text),
            Self::Rich(value) => {
                for run in &mut value.runs {
                    unprotect(&mut run.text);
                }
            }
        }
    }
}

pub(crate) fn read_container<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    closing: &[u8],
    maximum: usize,
    preserve: bool,
) -> Result<ParsedText> {
    if !preserve {
        return read_plain_container(xml, depth, closing, maximum);
    }
    let mut value = RichText::default();
    let mut formatted = false;
    let mut plain = false;
    let mut plain_text = None;
    let mut payload = 0usize;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == depth + 1 => {
                match e.local_name().as_ref().as_bytes() {
                    b"t" => {
                        if plain || !value.runs.is_empty() {
                            return Err(invalid("Duplicate or mixed plain rich-text content"));
                        }
                        plain = true;
                        let text = read_text(xml, depth + 1, maximum)?;
                        payload = payload.saturating_add(text.len());
                        plain_text = Some(text);
                    }
                    b"r" => {
                        if plain {
                            return Err(invalid("Mixed plain and run content"));
                        }
                        formatted = true;
                        let run = read_run(xml, depth + 1, maximum)?;
                        payload = payload.saturating_add(run.text.len()).saturating_add(
                            run.font.as_ref().map_or(0, |f| {
                                size_of::<RunFont>() + f.name.as_ref().map_or(0, |n| n.len())
                            }),
                        );
                        reserve(
                            &mut value.runs,
                            payload,
                            value.phonetic_runs.capacity() * size_of::<PhoneticRun>(),
                            maximum,
                        )?;
                        value.runs.push(run);
                    }
                    b"rPh" => {
                        formatted = true;
                        let start = integer(&e, b"sb")?;
                        let end = integer(&e, b"eb")?;
                        if start > end {
                            return Err(invalid("Reversed phonetic source range"));
                        }
                        let text = read_phonetic(xml, depth + 1, maximum)?;
                        payload = payload.saturating_add(text.len());
                        reserve(
                            &mut value.phonetic_runs,
                            payload,
                            value.runs.capacity() * size_of::<RichTextRun>(),
                            maximum,
                        )?;
                        value.phonetic_runs.push(PhoneticRun { start, end, text });
                    }
                    b"phoneticPr" => {
                        if value.phonetic_properties.is_some() {
                            return Err(invalid("Duplicate phonetic properties"));
                        }
                        formatted = true;
                        let properties = PhoneticProperties {
                            font_id: integer(&e, b"fontId")?,
                            kind: attribute(&e, b"type")?.map(String::into_boxed_str),
                            alignment: attribute(&e, b"alignment")?.map(String::into_boxed_str),
                        };
                        validate_phonetic(&properties)?;
                        payload = payload
                            .saturating_add(size_of::<PhoneticProperties>())
                            .saturating_add(properties.kind.as_ref().map_or(0, |s| s.len()))
                            .saturating_add(properties.alignment.as_ref().map_or(0, |s| s.len()));
                        value.phonetic_properties = Some(Box::new(properties));
                        crate::formatting::consume_property(xml, depth + 1)?;
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Unknown rich-text element",
                        ));
                    }
                }
                if formatted
                    && size_of::<RichText>()
                        + value.runs.capacity() * size_of::<RichTextRun>()
                        + value.phonetic_runs.capacity() * size_of::<PhoneticRun>()
                        + payload
                        > maximum
                {
                    return Err(limit());
                }
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref().as_bytes() == closing =>
            {
                if let Some(text) = plain_text {
                    if !formatted {
                        crate::encode::validate_xml_text(&text)?;
                        return Ok(ParsedText::Plain(text));
                    }
                    reserve(
                        &mut value.runs,
                        payload,
                        value.phonetic_runs.capacity() * size_of::<PhoneticRun>(),
                        maximum,
                    )?;
                    value.runs.push(RichTextRun { text, font: None });
                }
                return if formatted {
                    validate(&value, maximum)?;
                    Ok(ParsedText::Rich(Box::new(value)))
                } else {
                    Ok(ParsedText::Plain("".into()))
                };
            }
            Event::Start(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown rich-text namespace or subtree",
                ));
            }
            Event::Text(t) if t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid rich-text container content")),
        }
    }
}
fn reserve<T>(values: &mut Vec<T>, payload: usize, other: usize, maximum: usize) -> Result<()> {
    let allowed = maximum
        .saturating_sub(size_of::<RichText>())
        .saturating_sub(payload)
        .saturating_sub(other)
        / size_of::<T>();
    if values.len() >= allowed {
        return Err(limit());
    }
    if values.len() == values.capacity() {
        let target = values.capacity().saturating_mul(2).max(4).min(allowed);
        values
            .try_reserve_exact(target - values.len())
            .map_err(|e| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot allocate rich-text runs",
                    e,
                )
            })?;
    }
    if values.capacity() > allowed {
        return Err(limit());
    }
    Ok(())
}
fn read_text<B: BufRead>(xml: &mut XmlStream<B>, depth: usize, maximum: usize) -> Result<Box<str>> {
    let mut text = String::new();
    append_text_element(xml, depth, &mut text, maximum)?;
    Ok(text.into_boxed_str())
}
fn read_run<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    maximum: usize,
) -> Result<RichTextRun> {
    let mut font = None;
    let mut text = None;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == depth + 1
                    && e.local_name().as_ref().as_bytes() == b"rPr" =>
            {
                if font.is_some() || text.is_some() {
                    return Err(invalid("Invalid run property order or duplicate"));
                }
                font = Some(Box::new(crate::formatting::read_font(
                    xml,
                    depth + 1,
                    maximum,
                    crate::formatting::FontContext::Run,
                )?));
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == depth + 1
                    && e.local_name().as_ref().as_bytes() == b"t" =>
            {
                if text.is_some() {
                    return Err(invalid("Duplicate run text"));
                }
                text = Some(read_text(xml, depth + 1, maximum)?);
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref().as_bytes() == b"r" =>
            {
                return Ok(RichTextRun {
                    text: text.unwrap_or_default(),
                    font,
                });
            }
            Event::Start(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown rich-text namespace or subtree",
                ));
            }
            Event::Text(t) if t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid display run content")),
        }
    }
}
fn read_phonetic<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    maximum: usize,
) -> Result<Box<str>> {
    let mut text = None;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == depth + 1
                    && e.local_name().as_ref().as_bytes() == b"t" =>
            {
                if text.is_some() {
                    return Err(invalid("Duplicate phonetic text"));
                }
                text = Some(read_text(xml, depth + 1, maximum)?);
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref().as_bytes() == b"rPh" =>
            {
                return Ok(text.unwrap_or_default());
            }
            Event::Start(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown rich-text namespace or subtree",
                ));
            }
            Event::Text(t) if t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid phonetic run content")),
        }
    }
}
fn integer(e: &BytesStart<'_>, key: &[u8]) -> Result<u32> {
    required_attribute(e, key)?
        .parse()
        .map_err(|_| invalid("Invalid rich-text integer attribute"))
}
fn validate_phonetic(p: &PhoneticProperties) -> Result<()> {
    if p.kind.as_deref().is_some_and(|s| {
        !matches!(
            s,
            "halfwidthKatakana" | "fullwidthKatakana" | "Hiragana" | "noConversion"
        )
    }) {
        return Err(invalid("Invalid phonetic type"));
    }
    if p.alignment
        .as_deref()
        .is_some_and(|s| !matches!(s, "noControl" | "left" | "center" | "distributed"))
    {
        return Err(invalid("Invalid phonetic alignment"));
    }
    Ok(())
}
pub(crate) fn validate(value: &RichText, maximum: usize) -> Result<()> {
    if value.memory_bytes() > maximum {
        return Err(limit());
    }
    for run in &value.runs {
        crate::encode::validate_xml_text(&run.text)?;
        if let Some(font) = &run.font {
            crate::formatting::validate_font(font)?;
        }
    }
    for run in &value.phonetic_runs {
        if run.start > run.end {
            return Err(invalid("Reversed phonetic range"));
        }
        crate::encode::validate_xml_text(&run.text)?;
    }
    if let Some(p) = &value.phonetic_properties {
        validate_phonetic(p)?;
    }
    Ok(())
}
pub(crate) fn write_container(output: &mut impl Write, value: &RichText) -> io::Result<()> {
    output.write_all(b"<is>")?;
    for run in &value.runs {
        output.write_all(b"<r>")?;
        if let Some(font) = &run.font {
            crate::formatting::write_font(output, font, crate::formatting::FontContext::Run)?;
        }
        output.write_all(b"<t xml:space=\"preserve\">")?;
        crate::encode::write_text(output, &run.text)?;
        output.write_all(b"</t></r>")?;
    }
    for run in &value.phonetic_runs {
        write!(
            output,
            "<rPh sb=\"{}\" eb=\"{}\"><t xml:space=\"preserve\">",
            run.start, run.end
        )?;
        crate::encode::write_text(output, &run.text)?;
        output.write_all(b"</t></rPh>")?;
    }
    if let Some(p) = &value.phonetic_properties {
        write!(output, "<phoneticPr fontId=\"{}\"", p.font_id)?;
        if let Some(kind) = &p.kind {
            crate::encode::write_attribute(output, "type", kind)?;
        }
        if let Some(align) = &p.alignment {
            crate::encode::write_attribute(output, "alignment", align)?;
        }
        output.write_all(b"/>")?;
    }
    output.write_all(b"</is>")
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn limit() -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "Rich-text allocation exceeds cell byte limit",
    )
}

fn read_plain_container<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    closing: &[u8],
    maximum: usize,
) -> Result<ParsedText> {
    let mut text = String::new();
    let mut plain = false;
    let mut runs = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == depth + 1 => {
                match e.local_name().as_ref().as_bytes() {
                    b"t" => {
                        if plain || runs {
                            return Err(invalid("Duplicate or mixed plain rich-text content"));
                        }
                        plain = true;
                        append_text_element(xml, depth + 1, &mut text, maximum)?;
                    }
                    b"r" => {
                        if plain {
                            return Err(invalid("Mixed plain and run content"));
                        }
                        runs = true;
                        append_plain_run(xml, depth + 1, &mut text, maximum)?;
                    }
                    b"rPh" | b"phoneticPr" => skip_subtree(xml, depth + 1)?,
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Unknown rich-text element",
                        ));
                    }
                }
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref().as_bytes() == closing =>
            {
                crate::encode::validate_xml_text(&text)?;
                return Ok(ParsedText::Plain(text.into_boxed_str()));
            }
            Event::Start(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown rich-text namespace or subtree",
                ));
            }
            Event::Text(t) if t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid rich-text container content")),
        }
    }
}
fn append_plain_run<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    text: &mut String,
    maximum: usize,
) -> Result<()> {
    let mut seen_text = false;
    let mut seen_properties = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == depth + 1 => {
                match e.local_name().as_ref().as_bytes() {
                    b"t" => {
                        if seen_text {
                            return Err(invalid("Duplicate run text"));
                        }
                        seen_text = true;
                        append_text_element(xml, depth + 1, text, maximum)?;
                    }
                    b"rPr" => {
                        if seen_text || seen_properties {
                            return Err(invalid("Invalid run property order or duplicate"));
                        }
                        seen_properties = true;
                        skip_subtree(xml, depth + 1)?;
                    }
                    _ => return Err(invalid("Invalid display run element")),
                }
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref().as_bytes() == b"r" =>
            {
                return Ok(());
            }
            Event::Start(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown rich-text namespace or subtree",
                ));
            }
            Event::Text(t) if t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid display run content")),
        }
    }
}
fn append_text_element<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    text: &mut String,
    maximum: usize,
) -> Result<()> {
    loop {
        let frame = xml.next()?;
        match frame.event {
            event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_))
                if frame.depth == depth =>
            {
                append_xml_text(text, &event, maximum)?
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref().as_bytes() == b"t" =>
            {
                return Ok(());
            }
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid rich-text character content")),
        }
    }
}
fn skip_subtree<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<()> {
    loop {
        let f = xml.next()?;
        match f.event {
            Event::End(_) if f.depth + 1 == depth => return Ok(()),
            Event::Eof => return Err(invalid("Incomplete rich-text subtree")),
            _ => {}
        }
    }
}

pub(crate) fn write_stored(output: &mut impl Write, value: &RichText) -> io::Result<()> {
    write!(output, "<entry xmlns=\"{}\">", crate::xml::MAIN_URI)?;
    write_container(output, value)?;
    output.write_all(b"</entry>")
}
