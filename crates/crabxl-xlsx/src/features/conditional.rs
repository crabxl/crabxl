//! `<conditionalFormatting>` codec.
use crate::encode::{write_attribute, write_text};
use crate::formatting::{read_color, write_color};
use crate::metadata::{attributes, boolean};
use crate::printing::namespace;
use crate::style_codec::skip;
use crate::xml::{Scope, XmlStream, append_xml_text};
use crabxl_core::features::{
    CfValueObject, CfValueType, ColorScale, ConditionalFormatting, ConditionalOperator,
    ConditionalRule, ConditionalRuleType, ConditionalTimePeriod, DataBar, IconSet, IconSetType,
};
use crabxl_core::{CellRange, Color, Result};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

const MAX_TEXT: usize = 1 << 20;

pub(crate) fn parse_sqref(value: &str) -> Result<Vec<CellRange>> {
    value.split_ascii_whitespace().map(str::parse).collect()
}
pub(crate) fn write_sqref(out: &mut impl Write, ranges: &[CellRange]) -> io::Result<()> {
    let mut text = String::new();
    for (i, r) in ranges.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        text.push_str(&r.to_string());
    }
    write_attribute(out, "sqref", &text)
}
pub(crate) fn parse_u32(value: &str) -> Result<u32> {
    value
        .trim()
        .parse()
        .map_err(|_| super::invalid("Invalid unsigned integer attribute"))
}
pub(crate) fn flag(out: &mut impl Write, name: &str, value: Option<bool>) -> io::Result<()> {
    if let Some(v) = value {
        write_attribute(out, name, if v { "1" } else { "0" })?;
    }
    Ok(())
}
pub(crate) fn number(
    out: &mut impl Write,
    name: &str,
    value: Option<impl ToString>,
) -> io::Result<()> {
    if let Some(v) = value {
        write_attribute(out, name, &v.to_string())?;
    }
    Ok(())
}
pub(crate) fn unknown_attribute() -> crabxl_core::Error {
    super::unsupported("Unsupported attribute on a feature element")
}
pub(crate) fn unknown_element() -> crabxl_core::Error {
    super::unsupported("Unsupported element inside a feature element")
}
/// Read the text content of an element whose `Start` (at `depth`) was consumed.
pub(crate) fn read_text<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<String> {
    let mut text = String::new();
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::End(_) if frame.depth + 1 == depth => return Ok(text),
            Event::Start(_) => return Err(unknown_element()),
            Event::Eof => return Err(super::invalid("Incomplete XML element")),
            Event::Comment(_) | Event::PI(_) => {}
            ref e => append_xml_text(&mut text, e, MAX_TEXT)?,
        }
    }
}
fn parse_enum<T>(value: &str, parse: fn(&str) -> Option<T>) -> Result<T> {
    parse(value).ok_or_else(|| super::unsupported("Unsupported enumerated attribute value"))
}

fn read_cfvo<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<CfValueObject> {
    let mut v = CfValueObject::default();
    attributes(start, |name, value| {
        match name {
            b"type" => v.value_type = Some(parse_enum(value, CfValueType::parse)?),
            b"val" => v.value = Some(value.into()),
            b"gte" => v.gte = Some(boolean(value)?),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    skip_empty(xml, depth)?;
    Ok(v)
}
/// Consume to the matching End; any child element is unsupported.
fn skip_empty<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<()> {
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::End(_) if frame.depth + 1 == depth => return Ok(()),
            Event::Start(_) => return Err(unknown_element()),
            Event::Eof => return Err(super::invalid("Incomplete XML element")),
            _ => {}
        }
    }
}

/// Which child of the container is being read.
enum Child {
    Cfvo(BytesStart<'static>, usize),
    Color(BytesStart<'static>, usize),
}
/// Iterate children of an element; calls `each` for every `cfvo`/`color`.
fn read_children<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    mut each: impl FnMut(&mut XmlStream<B>, Child) -> Result<()>,
) -> Result<()> {
    loop {
        let frame = xml.next()?;
        let (d, scope) = (frame.depth, frame.scope);
        match frame.event {
            Event::End(_) if d + 1 == depth => return Ok(()),
            Event::Start(e) => {
                if scope != Scope::Spreadsheet {
                    return Err(unknown_element());
                }
                let e = e.into_owned();
                match e.local_name().as_ref().as_bytes() {
                    b"cfvo" => each(xml, Child::Cfvo(e, d))?,
                    b"color" => each(xml, Child::Color(e, d))?,
                    _ => return Err(unknown_element()),
                }
            }
            Event::Eof => return Err(super::invalid("Incomplete XML element")),
            Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                return Err(super::invalid("Unexpected text"));
            }
            _ => {}
        }
    }
}
fn color_child<B: BufRead>(xml: &mut XmlStream<B>, e: &BytesStart<'_>, d: usize) -> Result<Color> {
    let color = read_color(e)?;
    skip(xml, d)?;
    Ok(color)
}

fn read_rule<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<ConditionalRule> {
    let mut r = ConditionalRule::default();
    attributes(start, |name, value| {
        match name {
            b"type" => r.rule_type = Some(parse_enum(value, ConditionalRuleType::parse)?),
            b"dxfId" => r.dxf_id = Some(parse_u32(value)?),
            b"priority" => r.priority = Some(parse_u32(value)?),
            b"stopIfTrue" => r.stop_if_true = Some(boolean(value)?),
            b"aboveAverage" => r.above_average = Some(boolean(value)?),
            b"percent" => r.percent = Some(boolean(value)?),
            b"bottom" => r.bottom = Some(boolean(value)?),
            b"operator" => r.operator = Some(parse_enum(value, ConditionalOperator::parse)?),
            b"text" => r.text = Some(value.into()),
            b"timePeriod" => r.time_period = Some(parse_enum(value, ConditionalTimePeriod::parse)?),
            b"rank" => r.rank = Some(parse_u32(value)?),
            b"stdDev" => {
                r.std_dev = Some(
                    value
                        .trim()
                        .parse()
                        .map_err(|_| super::invalid("Invalid stdDev"))?,
                )
            }
            b"equalAverage" => r.equal_average = Some(boolean(value)?),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    loop {
        let frame = xml.next()?;
        let (d, scope) = (frame.depth, frame.scope);
        match frame.event {
            Event::End(_) if d + 1 == depth => return Ok(r),
            Event::Start(e) => {
                if scope != Scope::Spreadsheet {
                    return Err(unknown_element());
                }
                let e = e.into_owned();
                match e.local_name().as_ref().as_bytes() {
                    b"formula" => r.formulas.push(read_text(xml, d)?.into()),
                    b"colorScale" if r.color_scale.is_none() => {
                        reject_attributes(&e)?;
                        let mut cs = ColorScale::default();
                        read_children(xml, d, |xml, c| {
                            match c {
                                Child::Cfvo(e, d) => cs.cfvos.push(read_cfvo(xml, &e, d)?),
                                Child::Color(e, d) => cs.colors.push(color_child(xml, &e, d)?),
                            }
                            Ok(())
                        })?;
                        r.color_scale = Some(cs);
                    }
                    b"dataBar" if r.data_bar.is_none() => {
                        let mut db = DataBar::default();
                        attributes(&e, |name, value| {
                            match name {
                                b"minLength" => db.min_length = Some(parse_u32(value)?),
                                b"maxLength" => db.max_length = Some(parse_u32(value)?),
                                b"showValue" => db.show_value = Some(boolean(value)?),
                                _ => return Err(unknown_attribute()),
                            }
                            Ok(())
                        })?;
                        read_children(xml, d, |xml, c| {
                            match c {
                                Child::Cfvo(e, d) => db.cfvos.push(read_cfvo(xml, &e, d)?),
                                Child::Color(e, d) => {
                                    if db.color.is_some() {
                                        return Err(unknown_element());
                                    }
                                    db.color = Some(color_child(xml, &e, d)?)
                                }
                            }
                            Ok(())
                        })?;
                        r.data_bar = Some(db);
                    }
                    b"iconSet" if r.icon_set.is_none() => {
                        let mut is = IconSet::default();
                        attributes(&e, |name, value| {
                            match name {
                                b"iconSet" => {
                                    is.icon_set = Some(parse_enum(value, IconSetType::parse)?)
                                }
                                b"showValue" => is.show_value = Some(boolean(value)?),
                                b"percent" => is.percent = Some(boolean(value)?),
                                b"reverse" => is.reverse = Some(boolean(value)?),
                                _ => return Err(unknown_attribute()),
                            }
                            Ok(())
                        })?;
                        read_children(xml, d, |xml, c| {
                            match c {
                                Child::Cfvo(e, d) => is.cfvos.push(read_cfvo(xml, &e, d)?),
                                Child::Color(..) => return Err(unknown_element()),
                            }
                            Ok(())
                        })?;
                        r.icon_set = Some(is);
                    }
                    b"extLst" if r.ext_list.is_none() => {
                        r.ext_list = Some(super::read_raw(xml, &e, d)?);
                    }
                    _ => return Err(unknown_element()),
                }
            }
            Event::Eof => return Err(super::invalid("Incomplete XML element")),
            Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                return Err(super::invalid("Unexpected text"));
            }
            _ => {}
        }
    }
}
fn reject_attributes(e: &BytesStart<'_>) -> Result<()> {
    attributes(e, |_, _| Err(unknown_attribute()))
}

pub(crate) fn read<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<ConditionalFormatting> {
    let mut cf = ConditionalFormatting::default();
    attributes(start, |name, value| {
        match name {
            b"sqref" => cf.ranges = parse_sqref(value)?,
            b"pivot" => cf.pivot = Some(boolean(value)?),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    loop {
        let frame = xml.next()?;
        let (d, scope) = (frame.depth, frame.scope);
        match frame.event {
            Event::End(_) if d + 1 == depth => return Ok(cf),
            Event::Start(e) => {
                if scope != Scope::Spreadsheet || e.local_name().as_ref().as_bytes() != b"cfRule" {
                    return Err(unknown_element());
                }
                let e = e.into_owned();
                cf.rules.push(read_rule(xml, &e, d)?);
            }
            Event::Eof => return Err(super::invalid("Incomplete XML element")),
            Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                return Err(super::invalid("Unexpected text"));
            }
            _ => {}
        }
    }
}

fn write_cfvo(out: &mut impl Write, v: &CfValueObject, uri: Option<&str>) -> io::Result<()> {
    out.write_all(b"<cfvo")?;
    namespace(out, uri)?;
    if let Some(t) = v.value_type {
        write_attribute(out, "type", t.as_str())?;
    }
    if let Some(val) = &v.value {
        write_attribute(out, "val", val)?;
    }
    flag(out, "gte", v.gte)?;
    out.write_all(b"/>")
}
fn write_cfvos(out: &mut impl Write, v: &[CfValueObject], uri: Option<&str>) -> io::Result<()> {
    v.iter().try_for_each(|c| write_cfvo(out, c, uri))
}
fn write_ns_color(out: &mut impl Write, c: &Color, uri: Option<&str>) -> io::Result<()> {
    let mut buf = Vec::new();
    write_color(&mut buf, "color", c)?;
    // Insert the namespace declaration after the element name.
    if let Some(u) = uri {
        out.write_all(b"<color")?;
        namespace(out, Some(u))?;
        out.write_all(&buf[6..])
    } else {
        out.write_all(&buf)
    }
}
fn write_rule(out: &mut impl Write, r: &ConditionalRule, uri: Option<&str>) -> io::Result<()> {
    out.write_all(b"<cfRule")?;
    namespace(out, uri)?;
    if let Some(t) = r.rule_type {
        write_attribute(out, "type", t.as_str())?;
    }
    number(out, "dxfId", r.dxf_id)?;
    number(out, "priority", r.priority)?;
    flag(out, "stopIfTrue", r.stop_if_true)?;
    flag(out, "aboveAverage", r.above_average)?;
    flag(out, "percent", r.percent)?;
    flag(out, "bottom", r.bottom)?;
    if let Some(o) = r.operator {
        write_attribute(out, "operator", o.as_str())?;
    }
    if let Some(t) = &r.text {
        write_attribute(out, "text", t)?;
    }
    if let Some(t) = r.time_period {
        write_attribute(out, "timePeriod", t.as_str())?;
    }
    number(out, "rank", r.rank)?;
    number(out, "stdDev", r.std_dev)?;
    flag(out, "equalAverage", r.equal_average)?;
    out.write_all(b">")?;
    for f in &r.formulas {
        out.write_all(b"<formula")?;
        namespace(out, uri)?;
        out.write_all(b">")?;
        write_text(out, f)?;
        out.write_all(b"</formula>")?;
    }
    if let Some(cs) = &r.color_scale {
        out.write_all(b"<colorScale")?;
        namespace(out, uri)?;
        out.write_all(b">")?;
        write_cfvos(out, &cs.cfvos, uri)?;
        for c in &cs.colors {
            write_ns_color(out, c, uri)?;
        }
        out.write_all(b"</colorScale>")?;
    }
    if let Some(db) = &r.data_bar {
        out.write_all(b"<dataBar")?;
        namespace(out, uri)?;
        number(out, "minLength", db.min_length)?;
        number(out, "maxLength", db.max_length)?;
        flag(out, "showValue", db.show_value)?;
        out.write_all(b">")?;
        write_cfvos(out, &db.cfvos, uri)?;
        if let Some(c) = &db.color {
            write_ns_color(out, c, uri)?;
        }
        out.write_all(b"</dataBar>")?;
    }
    if let Some(is) = &r.icon_set {
        out.write_all(b"<iconSet")?;
        namespace(out, uri)?;
        if let Some(t) = is.icon_set {
            write_attribute(out, "iconSet", t.as_str())?;
        }
        flag(out, "showValue", is.show_value)?;
        flag(out, "percent", is.percent)?;
        flag(out, "reverse", is.reverse)?;
        out.write_all(b">")?;
        write_cfvos(out, &is.cfvos, uri)?;
        out.write_all(b"</iconSet>")?;
    }
    if let Some(raw) = &r.ext_list {
        out.write_all(raw.as_bytes())?;
    }
    out.write_all(b"</cfRule>")
}
/// Write all blocks; writes nothing for an empty slice.
pub(crate) fn write(
    out: &mut impl Write,
    formats: &[ConditionalFormatting],
    uri: Option<&str>,
) -> io::Result<()> {
    for cf in formats {
        out.write_all(b"<conditionalFormatting")?;
        namespace(out, uri)?;
        write_sqref(out, &cf.ranges)?;
        flag(out, "pivot", cf.pivot)?;
        out.write_all(b">")?;
        for r in &cf.rules {
            write_rule(out, r, uri)?;
        }
        out.write_all(b"</conditionalFormatting>")?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::xml::{MAIN_URI, STRICT_MAIN_URI};
    use crabxl_core::ResourceLimits;
    use std::io::Cursor;

    const SAMPLES: &[&str] = &[
        r#"<conditionalFormatting sqref="A1:A10"><cfRule type="cellIs" priority="1" operator="between" dxfId="0"><formula>1</formula><formula>5</formula></cfRule></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="B1:B10 D1:D3"><cfRule type="expression" priority="2" dxfId="1" stopIfTrue="1"><formula>ISBLANK(B1)</formula></cfRule></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="C1:C10"><cfRule type="colorScale" priority="3"><colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="00FF0000"/><color rgb="0000FF00"/><color rgb="000000FF"/></colorScale></cfRule></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="E1:E10"><cfRule type="dataBar" priority="4"><dataBar minLength="5" maxLength="90" showValue="0"><cfvo type="num" val="0"/><cfvo type="max"/><color rgb="00638EC6"/></dataBar></cfRule></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="F1:F10"><cfRule type="iconSet" priority="5"><iconSet iconSet="5Arrows" reverse="1"><cfvo type="percent" val="1"/><cfvo type="percent" val="20"/><cfvo type="percent" val="40"/><cfvo type="percent" val="60"/><cfvo type="percent" val="80"/></iconSet></cfRule></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="G1:G10"><cfRule type="top10" rank="5" priority="6" percent="1" bottom="1"/></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="H1:H10"><cfRule type="aboveAverage" priority="7" equalAverage="1" aboveAverage="0" stdDev="1"/></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="I1:I10"><cfRule type="duplicateValues" priority="8"/></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="J1:J10"><cfRule type="containsText" priority="9" operator="containsText" text="a&amp;b&lt;"><formula>NOT(ISERROR(SEARCH("a&amp;b&lt;",J1)))</formula></cfRule></conditionalFormatting>"#,
        r#"<conditionalFormatting sqref="K1:K10"><cfRule type="timePeriod" priority="10" timePeriod="lastWeek"><formula>AND(K1&gt;1,K1&lt;2)</formula></cfRule></conditionalFormatting>"#,
    ];

    fn parse_all(doc: &str) -> Result<Vec<ConditionalFormatting>> {
        let mut xml = XmlStream::new(
            Cursor::new(doc.as_bytes().to_vec()),
            "t".into(),
            doc.len() as u64,
            ResourceLimits::default(),
        );
        let mut out = Vec::new();
        loop {
            let frame = xml.next()?;
            let depth = frame.depth;
            match frame.event {
                Event::Start(e)
                    if e.local_name().as_ref().as_bytes() == b"conditionalFormatting" =>
                {
                    let e = e.into_owned();
                    out.push(read(&mut xml, &e, depth)?);
                }
                Event::Eof => return Ok(out),
                _ => {}
            }
        }
    }
    fn wrap(body: &str, ns: &str) -> String {
        format!("<worksheet xmlns=\"{ns}\">{body}</worksheet>")
    }

    #[test]
    fn openpyxl_samples_round_trip() {
        let all = SAMPLES.concat();
        let parsed = parse_all(&wrap(&all, MAIN_URI)).unwrap();
        assert_eq!(parsed.len(), SAMPLES.len());
        let mut out = Vec::new();
        write(&mut out, &parsed, None).unwrap();
        let again = parse_all(&wrap(std::str::from_utf8(&out).unwrap(), MAIN_URI)).unwrap();
        assert_eq!(parsed, again);
    }
    #[test]
    fn fields_are_decoded() {
        let p = parse_all(&wrap(SAMPLES[1], MAIN_URI)).unwrap();
        assert_eq!(p[0].ranges.len(), 2);
        let r = &p[0].rules[0];
        assert_eq!(r.rule_type, Some(ConditionalRuleType::Expression));
        assert_eq!(r.dxf_id, Some(1));
        assert_eq!(r.stop_if_true, Some(true));
        assert_eq!(&*r.formulas[0], "ISBLANK(B1)");
        let p = parse_all(&wrap(SAMPLES[8], MAIN_URI)).unwrap();
        assert_eq!(
            &*p[0].rules[0].formulas[0],
            "NOT(ISERROR(SEARCH(\"a&b<\",J1)))"
        );
        assert_eq!(p[0].rules[0].text.as_deref(), Some("a&b<"));
    }
    #[test]
    fn strict_namespace_round_trips_with_declared_namespace() {
        let all = SAMPLES.concat();
        let parsed = parse_all(&wrap(&all, STRICT_MAIN_URI)).unwrap();
        assert_eq!(parsed.len(), SAMPLES.len());
        let mut out = Vec::new();
        write(&mut out, &parsed, Some(STRICT_MAIN_URI)).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(STRICT_MAIN_URI));
        let again = parse_all(&format!("<worksheet>{text}</worksheet>")).unwrap();
        assert_eq!(parsed, again);
    }
    #[test]
    fn unknown_items_are_unsupported() {
        for body in [
            "<conditionalFormatting sqref=\"A1\"><bogus/></conditionalFormatting>",
            "<conditionalFormatting sqref=\"A1\" x=\"1\"/>",
            "<conditionalFormatting sqref=\"A1\"><cfRule type=\"expression\" zz=\"1\"/></conditionalFormatting>",
            "<conditionalFormatting sqref=\"A1\"><cfRule type=\"dataBar\"><dataBar><cfvo type=\"min\"/><cfvo type=\"max\"/><extLst/></dataBar></cfRule></conditionalFormatting>",
            "<conditionalFormatting sqref=\"A1\"><cfRule type=\"nope\"/></conditionalFormatting>",
        ] {
            let error = parse_all(&wrap(body, MAIN_URI)).unwrap_err();
            assert_eq!(error.kind(), crabxl_core::ErrorKind::Unsupported, "{body}");
        }
    }
    #[test]
    fn malformed_input_is_an_error() {
        assert!(parse_all(&wrap("<conditionalFormatting sqref=\"A1\"><cfRule priority=\"x\"/></conditionalFormatting>", MAIN_URI)).is_err());
        assert!(parse_all(&wrap("<conditionalFormatting sqref=\"A1:\"/>", MAIN_URI)).is_err());
    }
    #[test]
    fn empty_slice_writes_nothing() {
        let mut out = Vec::new();
        write(&mut out, &[], None).unwrap();
        assert!(out.is_empty());
    }
}

#[cfg(test)]
mod ext_tests {
    #![allow(clippy::unwrap_used)]
    #[test]
    fn keeps_rule_extension_list() {
        let source = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><conditionalFormatting sqref="A1:A9"><cfRule type="dataBar" priority="1"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar><extLst><ext uri="{B025F937-C7B1-47D3-B67F-A62EFF666E3E}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:id>{AAAAAAAA-0000-0000-0000-000000000001}</x14:id></ext></extLst></cfRule></conditionalFormatting></worksheet>"#;
        let features = crate::feature_xml::sheet_features_from_xml(source.as_bytes(), &[]).unwrap();
        let raw = features.conditional_formats[0].rules[0]
            .ext_list
            .as_deref()
            .unwrap();
        assert!(raw.contains("x14:id"));
        let xml = crate::feature_xml::sheet_features_to_xml(&features).unwrap();
        let back = crate::feature_xml::sheet_features_from_xml(&xml.worksheet, &[]).unwrap();
        assert_eq!(back, features);
    }
}
