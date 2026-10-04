// SPDX-License-Identifier: MIT
// Selected style layouts adapted from umya-spreadsheet, Copyright (c) 2020 MathNya.
// Shared streaming codecs and validation use the canonical core components.
use crabxl_core::{Alignment, Border, Error, ErrorKind, Fill, Protection, Result};
use std::io::{self, Write};
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
pub(crate) fn validate_fill(value: &Fill) -> Result<()> {
    value.validate()
}
pub(crate) fn validate_border(value: &Border) -> Result<()> {
    value.validate()
}
pub(crate) fn validate_alignment(value: &Alignment) -> Result<()> {
    value.validate()
}
pub(crate) fn write_fill(out: &mut impl Write, fill: &Fill) -> io::Result<()> {
    out.write_all(b"<fill>")?;
    match fill {
        Fill::Pattern(v) => {
            out.write_all(b"<patternFill")?;
            if let Some(pattern) = v.pattern {
                write!(out, " patternType=\"{}\"", pattern.as_str())?;
            }
            out.write_all(b">")?;
            if let Some(c) = v.foreground {
                crate::formatting::write_color(out, "fgColor", c)?;
            }
            if let Some(c) = v.background {
                crate::formatting::write_color(out, "bgColor", c)?;
            }
            out.write_all(b"</patternFill>")?;
        }
        Fill::Gradient(v) => {
            out.write_all(b"<gradientFill")?;
            if let Some(kind) = v.kind {
                write!(out, " type=\"{}\"", kind.as_str())?;
            }
            if let Some(degree) = v.degree {
                write!(out, " degree=\"{degree}\"")?;
            }
            for (key, val) in ["left", "right", "top", "bottom"].into_iter().zip(v.edges) {
                if let Some(val) = val {
                    write!(out, " {key}=\"{val}\"")?;
                }
            }
            out.write_all(b">")?;
            for stop in &v.stops {
                write!(out, "<stop position=\"{}\">", stop.position)?;
                crate::formatting::write_color(out, "color", stop.color)?;
                out.write_all(b"</stop>")?;
            }
            out.write_all(b"</gradientFill>")?;
        }
    }
    out.write_all(b"</fill>")
}
pub(crate) fn write_border(out: &mut impl Write, border: &Border) -> io::Result<()> {
    out.write_all(b"<border")?;
    for (key, val) in [
        ("diagonalUp", border.diagonal_up),
        ("diagonalDown", border.diagonal_down),
        ("outline", border.outline),
    ] {
        if let Some(val) = val {
            write!(out, " {key}=\"{}\"", u8::from(val))?;
        }
    }
    out.write_all(b">")?;
    for (key, val) in [
        "left",
        "right",
        "top",
        "bottom",
        "diagonal",
        "vertical",
        "horizontal",
        "start",
        "end",
    ]
    .into_iter()
    .zip(border.sides)
    {
        if let Some(val) = val {
            write!(out, "<{key}")?;
            if let Some(line) = val.line {
                write!(out, " style=\"{}\"", line.as_str())?;
            }
            out.write_all(b">")?;
            if let Some(c) = val.color {
                crate::formatting::write_color(out, "color", c)?;
            }
            write!(out, "</{key}>")?;
        }
    }
    out.write_all(b"</border>")
}
pub(crate) fn write_alignment(
    out: &mut impl Write,
    v: &Alignment,
    policy: crate::StyleWritePolicy,
) -> io::Result<()> {
    let retain = policy == crate::StyleWritePolicy::RetainExplicit;
    out.write_all(b"<alignment")?;
    if let Some(n) = v.horizontal {
        write!(out, " horizontal=\"{}\"", n.as_str())?;
    }
    if let Some(n) = v.vertical {
        write!(out, " vertical=\"{}\"", n.as_str())?;
    }
    if let Some(n) = v.rotation.filter(|value| retain || *value != 0) {
        write!(out, " textRotation=\"{n}\"")?;
    }
    for (key, val) in [
        ("wrapText", v.wrap_text),
        ("shrinkToFit", v.shrink_to_fit),
        ("justifyLastLine", v.justify_last_line),
        ("mergeCell", v.merge_cell),
    ] {
        if let Some(n) = val.filter(|value| retain || *value) {
            write!(out, " {key}=\"{}\"", u8::from(n))?;
        }
    }
    for (key, val) in [
        ("indent", v.indent),
        ("relativeIndent", v.relative_indent),
        ("readingOrder", v.reading_order),
    ] {
        if let Some(n) = val.filter(|value| retain || *value != 0.0) {
            write!(out, " {key}=\"{n}\"")?;
        }
    }
    out.write_all(b"/>")
}
pub(crate) fn write_protection(out: &mut impl Write, v: &Protection) -> io::Result<()> {
    out.write_all(b"<protection")?;
    for (key, val) in [("locked", v.locked), ("hidden", v.hidden)] {
        if let Some(n) = val {
            write!(out, " {key}=\"{}\"", u8::from(n))?;
        }
    }
    out.write_all(b"/>")
}

use crate::xml::{Scope, XmlStream, attribute, required_attribute};
use crabxl_core::{
    BorderLine, BorderSide, Color, FillPattern, GradientFill, GradientKind, GradientStop,
    HorizontalAlignment, PatternFill, VerticalAlignment,
};
use quick_xml::{
    encoding::Decoder,
    events::{BytesStart, Event},
};
use std::io::BufRead;
pub(crate) fn skip<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<()> {
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::End(_) if frame.depth + 1 == depth => return Ok(()),
            Event::Eof => return Err(invalid("Incomplete style subtree")),
            _ => {}
        }
    }
}
fn blank(event: &Event<'_>) -> bool {
    matches!(event,Event::Text(t) if t.iter().all(u8::is_ascii_whitespace))
        || matches!(event, Event::Comment(_) | Event::PI(_))
}
fn parse_float(e: &BytesStart<'_>, key: &[u8], decoder: Decoder) -> Result<Option<f64>> {
    attribute(e, key, decoder)?
        .map(|value| {
            value
                .parse()
                .map_err(|_| invalid("Invalid style numeric attribute"))
        })
        .transpose()
}
fn parse_bool(e: &BytesStart<'_>, key: &[u8], decoder: Decoder) -> Result<Option<bool>> {
    attribute(e, key, decoder)?
        .map(|value| crate::formatting::boolean(Some(&value)))
        .transpose()
}
pub(crate) fn read_alignment(e: &BytesStart<'_>, decoder: Decoder) -> Result<Alignment> {
    crate::formatting::check_attributes(
        e,
        &[
            b"horizontal",
            b"vertical",
            b"textRotation",
            b"wrapText",
            b"shrinkToFit",
            b"indent",
            b"relativeIndent",
            b"justifyLastLine",
            b"readingOrder",
            b"mergeCell",
        ],
    )?;
    let v = Alignment {
        horizontal: attribute(e, b"horizontal", decoder)?
            .map(|s| HorizontalAlignment::parse(&s))
            .transpose()?,
        vertical: attribute(e, b"vertical", decoder)?
            .map(|s| VerticalAlignment::parse(&s))
            .transpose()?,
        rotation: attribute(e, b"textRotation", decoder)?
            .map(|s| s.parse().map_err(|_| invalid("Invalid text rotation")))
            .transpose()?,
        wrap_text: parse_bool(e, b"wrapText", decoder)?,
        shrink_to_fit: parse_bool(e, b"shrinkToFit", decoder)?,
        indent: parse_float(e, b"indent", decoder)?,
        relative_indent: parse_float(e, b"relativeIndent", decoder)?,
        justify_last_line: parse_bool(e, b"justifyLastLine", decoder)?,
        reading_order: parse_float(e, b"readingOrder", decoder)?,
        merge_cell: parse_bool(e, b"mergeCell", decoder)?,
    };
    validate_alignment(&v)?;
    Ok(v)
}
pub(crate) fn read_protection(e: &BytesStart<'_>, decoder: Decoder) -> Result<Protection> {
    crate::formatting::check_attributes(e, &[b"locked", b"hidden"])?;
    Ok(Protection {
        locked: parse_bool(e, b"locked", decoder)?,
        hidden: parse_bool(e, b"hidden", decoder)?,
    })
}
pub(crate) fn read_color(e: &BytesStart<'_>, decoder: Decoder) -> Result<Color> {
    crate::formatting::check_attributes(e, &[b"rgb", b"theme", b"indexed", b"auto", b"tint"])?;
    crate::formatting::read_color(e, decoder)
}
pub(crate) fn read_fill<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    maximum: usize,
) -> Result<Fill> {
    let mut fill = None;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == depth + 1 => {
                if fill.is_some() {
                    return Err(invalid("Duplicate or mixed fill content"));
                }
                fill = Some(match e.local_name().as_ref() {
                    b"patternFill" => {
                        crate::formatting::check_attributes(&e, &[b"patternType"])?;
                        let mut v = PatternFill {
                            pattern: attribute(&e, b"patternType", frame.decoder)?
                                .map(|s| FillPattern::parse(&s))
                                .transpose()?,
                            ..Default::default()
                        };
                        loop {
                            let child = xml.next()?;
                            match child.event {
                                Event::Start(color)
                                    if child.scope == Scope::Spreadsheet
                                        && child.depth == depth + 2
                                        && matches!(
                                            color.local_name().as_ref(),
                                            b"fgColor" | b"bgColor"
                                        ) =>
                                {
                                    let target = if color.local_name().as_ref() == b"fgColor" {
                                        &mut v.foreground
                                    } else {
                                        &mut v.background
                                    };
                                    if target.is_some() {
                                        return Err(invalid("Duplicate pattern color"));
                                    }
                                    *target = Some(read_color(&color, child.decoder)?);
                                    crate::formatting::consume_property(xml, depth + 2)?;
                                }
                                Event::End(end)
                                    if child.scope == Scope::Spreadsheet
                                        && child.depth == depth
                                        && end.local_name().as_ref() == b"patternFill" =>
                                {
                                    break;
                                }
                                ref event if blank(event) => {}
                                _ => return Err(invalid("Invalid pattern fill content")),
                            }
                        }
                        Fill::Pattern(v)
                    }
                    b"gradientFill" => {
                        crate::formatting::check_attributes(
                            &e,
                            &[b"type", b"degree", b"left", b"right", b"top", b"bottom"],
                        )?;
                        let mut v = GradientFill {
                            kind: attribute(&e, b"type", frame.decoder)?
                                .map(|s| GradientKind::parse(&s))
                                .transpose()?,
                            degree: parse_float(&e, b"degree", frame.decoder)?,
                            edges: [
                                parse_float(&e, b"left", frame.decoder)?,
                                parse_float(&e, b"right", frame.decoder)?,
                                parse_float(&e, b"top", frame.decoder)?,
                                parse_float(&e, b"bottom", frame.decoder)?,
                            ],
                            stops: Vec::new(),
                        };
                        loop {
                            let child = xml.next()?;
                            match child.event {
                                Event::Start(stop)
                                    if child.scope == Scope::Spreadsheet
                                        && child.depth == depth + 2
                                        && stop.local_name().as_ref() == b"stop" =>
                                {
                                    crate::formatting::check_attributes(&stop, &[b"position"])?;
                                    let position =
                                        required_attribute(&stop, b"position", child.decoder)?
                                            .parse()
                                            .map_err(|_| invalid("Invalid gradient stop"))?;
                                    let mut color = None;
                                    loop {
                                        let detail = xml.next()?;
                                        match detail.event {
                                            Event::Start(e)
                                                if detail.scope == Scope::Spreadsheet
                                                    && detail.depth == depth + 3
                                                    && e.local_name().as_ref() == b"color" =>
                                            {
                                                if color.is_some() {
                                                    return Err(invalid(
                                                        "Duplicate gradient color",
                                                    ));
                                                }
                                                color = Some(read_color(&e, detail.decoder)?);
                                                crate::formatting::consume_property(
                                                    xml,
                                                    depth + 3,
                                                )?;
                                            }
                                            Event::End(e)
                                                if detail.scope == Scope::Spreadsheet
                                                    && detail.depth == depth + 1
                                                    && e.local_name().as_ref() == b"stop" =>
                                            {
                                                break;
                                            }
                                            ref event if blank(event) => {}
                                            _ => {
                                                return Err(invalid(
                                                    "Invalid gradient stop content",
                                                ));
                                            }
                                        }
                                    }
                                    let allowed = maximum.saturating_sub(size_of::<GradientFill>())
                                        / (size_of::<GradientStop>() + size_of::<u64>());
                                    if v.stops.len() >= allowed {
                                        return Err(Error::new(
                                            ErrorKind::LimitExceeded,
                                            "Gradient stop payload exceeds catalog allowance",
                                        ));
                                    }
                                    if v.stops.len() == v.stops.capacity() {
                                        let capacity = v
                                            .stops
                                            .capacity()
                                            .saturating_mul(2)
                                            .max(4)
                                            .min(allowed);
                                        v.stops
                                            .try_reserve_exact(capacity - v.stops.len())
                                            .map_err(|error| {
                                                Error::caused_by(
                                                    ErrorKind::MemoryBudgetExceeded,
                                                    "Cannot allocate gradient stops",
                                                    error,
                                                )
                                            })?;
                                        if v.stops.capacity() > allowed {
                                            return Err(Error::new(
                                                ErrorKind::LimitExceeded,
                                                "Gradient capacity exceeds catalog allowance",
                                            ));
                                        }
                                    }
                                    v.stops.push(GradientStop {
                                        position,
                                        color: color
                                            .ok_or_else(|| invalid("Gradient stop has no color"))?,
                                    });
                                }
                                Event::End(e)
                                    if child.scope == Scope::Spreadsheet
                                        && child.depth == depth
                                        && e.local_name().as_ref() == b"gradientFill" =>
                                {
                                    break;
                                }
                                ref event if blank(event) => {}
                                _ => return Err(invalid("Invalid gradient fill content")),
                            }
                        }
                        Fill::Gradient(v)
                    }
                    _ => return Err(Error::new(ErrorKind::Unsupported, "Unknown fill component")),
                });
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref() == b"fill" =>
            {
                let value = fill.unwrap_or(Fill::Pattern(PatternFill::default()));
                validate_fill(&value)?;
                return Ok(value);
            }
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid fill content")),
        }
    }
}
pub(crate) fn read_border_header(e: &BytesStart<'_>, decoder: Decoder) -> Result<Border> {
    crate::formatting::check_attributes(e, &[b"diagonalUp", b"diagonalDown", b"outline"])?;
    Ok(Border {
        sides: [None; 9],
        diagonal_up: parse_bool(e, b"diagonalUp", decoder)?,
        diagonal_down: parse_bool(e, b"diagonalDown", decoder)?,
        outline: parse_bool(e, b"outline", decoder)?,
    })
}
pub(crate) fn read_border<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    mut border: Border,
) -> Result<Border> {
    let keys: [&[u8]; 9] = [
        b"left",
        b"right",
        b"top",
        b"bottom",
        b"diagonal",
        b"vertical",
        b"horizontal",
        b"start",
        b"end",
    ];
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == depth + 1 => {
                let i = keys
                    .iter()
                    .position(|key| *key == e.local_name().as_ref())
                    .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Unknown border edge"))?;
                if border.sides[i].is_some() {
                    return Err(invalid("Duplicate border edge"));
                }
                crate::formatting::check_attributes(&e, &[b"style"])?;
                let mut side = BorderSide {
                    line: attribute(&e, b"style", frame.decoder)?
                        .map(|s| BorderLine::parse(&s))
                        .transpose()?,
                    color: None,
                };
                loop {
                    let child = xml.next()?;
                    match child.event {
                        Event::Start(e)
                            if child.scope == Scope::Spreadsheet
                                && child.depth == depth + 2
                                && e.local_name().as_ref() == b"color" =>
                        {
                            if side.color.is_some() {
                                return Err(invalid("Duplicate border color"));
                            }
                            side.color = Some(read_color(&e, child.decoder)?);
                            crate::formatting::consume_property(xml, depth + 2)?;
                        }
                        Event::End(e)
                            if child.scope == Scope::Spreadsheet
                                && child.depth == depth
                                && e.local_name().as_ref() == keys[i] =>
                        {
                            break;
                        }
                        ref event if blank(event) => {}
                        _ => return Err(invalid("Invalid border edge content")),
                    }
                }
                border.sides[i] = Some(side);
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth + 1 == depth
                    && e.local_name().as_ref() == b"border" =>
            {
                validate_border(&border)?;
                return Ok(border);
            }
            ref event if blank(event) => {}
            _ => return Err(invalid("Invalid border content")),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crabxl_core::ResourceLimits;
    use std::io::Cursor;

    #[test]
    fn gradient_validation_scratch_respects_the_component_allowance() {
        let source = format!(
            "<fill xmlns=\"{}\"><gradientFill><stop position=\"0\"><color rgb=\"FF000000\"/></stop><stop position=\"1\"><color rgb=\"FFFFFFFF\"/></stop></gradientFill></fill>",
            crate::xml::MAIN_URI
        );
        let parse = |allowance| {
            let mut xml = XmlStream::new(
                Cursor::new(source.as_bytes()),
                "styles.xml".into(),
                source.len() as u64,
                ResourceLimits::default(),
            );
            xml.next().unwrap();
            read_fill(&mut xml, 1, allowance)
        };
        let payload = size_of::<GradientFill>() + 2 * size_of::<GradientStop>();
        assert_eq!(parse(payload).unwrap_err().kind(), ErrorKind::LimitExceeded);
        let fill = parse(payload + 2 * size_of::<u64>()).unwrap();
        let Fill::Gradient(gradient) = fill else {
            panic!("Expected gradient");
        };
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[0].position, 0.0);
        assert_eq!(gradient.stops[1].position, 1.0);
    }
}
