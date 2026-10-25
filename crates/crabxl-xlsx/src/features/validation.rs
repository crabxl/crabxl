//! `<dataValidations>` codec.
use super::conditional::{
    flag, number, parse_sqref, parse_u32, read_text, unknown_attribute, unknown_element,
    write_sqref,
};
use crate::encode::{write_attribute, write_text};
use crate::metadata::{attributes, boolean};
use crate::printing::namespace;
use crate::xml::{Scope, XmlStream};
use crabxl_core::Result;
use crabxl_core::features::{
    DataValidation, DataValidations, ImeMode, ValidationErrorStyle, ValidationOperator,
    ValidationType,
};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

fn parse_enum<T>(value: &str, parse: fn(&str) -> Option<T>) -> Result<T> {
    parse(value).ok_or_else(|| super::unsupported("Unsupported enumerated attribute value"))
}

fn read_one<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<DataValidation> {
    let mut v = DataValidation::default();
    attributes(start, |name, value| {
        match name {
            b"sqref" => v.ranges = parse_sqref(value)?,
            b"type" => v.validation_type = Some(parse_enum(value, ValidationType::parse)?),
            b"operator" => v.operator = Some(parse_enum(value, ValidationOperator::parse)?),
            b"errorStyle" => v.error_style = Some(parse_enum(value, ValidationErrorStyle::parse)?),
            b"imeMode" => v.ime_mode = Some(parse_enum(value, ImeMode::parse)?),
            b"allowBlank" => v.allow_blank = Some(boolean(value)?),
            b"showDropDown" => v.show_drop_down = Some(boolean(value)?),
            b"showInputMessage" => v.show_input_message = Some(boolean(value)?),
            b"showErrorMessage" => v.show_error_message = Some(boolean(value)?),
            b"errorTitle" => v.error_title = Some(value.into()),
            b"error" => v.error = Some(value.into()),
            b"promptTitle" => v.prompt_title = Some(value.into()),
            b"prompt" => v.prompt = Some(value.into()),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    loop {
        let frame = xml.next()?;
        let (d, scope) = (frame.depth, frame.scope);
        match frame.event {
            Event::End(_) if d + 1 == depth => return Ok(v),
            Event::Start(e) => {
                if scope != Scope::Spreadsheet {
                    return Err(unknown_element());
                }
                let name = e.local_name().as_ref().as_bytes().to_vec();
                match name.as_slice() {
                    b"formula1" => v.formula1 = Some(read_text(xml, d)?.into()),
                    b"formula2" => v.formula2 = Some(read_text(xml, d)?.into()),
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

pub(crate) fn read<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<DataValidations> {
    let mut dv = DataValidations::default();
    attributes(start, |name, value| {
        match name {
            b"count" => {
                parse_u32(value)?;
            }
            b"disablePrompts" => dv.disable_prompts = Some(boolean(value)?),
            b"xWindow" => dv.x_window = Some(parse_u32(value)?),
            b"yWindow" => dv.y_window = Some(parse_u32(value)?),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    loop {
        let frame = xml.next()?;
        let (d, scope) = (frame.depth, frame.scope);
        match frame.event {
            Event::End(_) if d + 1 == depth => return Ok(dv),
            Event::Start(e) => {
                if scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"dataValidation"
                {
                    return Err(unknown_element());
                }
                let e = e.into_owned();
                dv.validations.push(read_one(xml, &e, d)?);
            }
            Event::Eof => return Err(super::invalid("Incomplete XML element")),
            Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                return Err(super::invalid("Unexpected text"));
            }
            _ => {}
        }
    }
}

fn text_attr(out: &mut impl Write, name: &str, v: &Option<Box<str>>) -> io::Result<()> {
    if let Some(v) = v {
        write_attribute(out, name, v)?;
    }
    Ok(())
}
fn formula(
    out: &mut impl Write,
    name: &str,
    v: &Option<Box<str>>,
    uri: Option<&str>,
) -> io::Result<()> {
    if let Some(v) = v {
        write!(out, "<{name}")?;
        namespace(out, uri)?;
        out.write_all(b">")?;
        write_text(out, v)?;
        write!(out, "</{name}>")?;
    }
    Ok(())
}
/// Write the container; writes nothing when there are no validations.
pub(crate) fn write(
    out: &mut impl Write,
    validations: &DataValidations,
    uri: Option<&str>,
) -> io::Result<()> {
    if validations.validations.is_empty() {
        return Ok(());
    }
    out.write_all(b"<dataValidations")?;
    namespace(out, uri)?;
    flag(out, "disablePrompts", validations.disable_prompts)?;
    number(out, "xWindow", validations.x_window)?;
    number(out, "yWindow", validations.y_window)?;
    write_attribute(out, "count", &validations.validations.len().to_string())?;
    out.write_all(b">")?;
    for v in &validations.validations {
        out.write_all(b"<dataValidation")?;
        namespace(out, uri)?;
        write_sqref(out, &v.ranges)?;
        if let Some(t) = v.validation_type {
            write_attribute(out, "type", t.as_str())?;
        }
        if let Some(t) = v.operator {
            write_attribute(out, "operator", t.as_str())?;
        }
        if let Some(t) = v.error_style {
            write_attribute(out, "errorStyle", t.as_str())?;
        }
        if let Some(t) = v.ime_mode {
            write_attribute(out, "imeMode", t.as_str())?;
        }
        flag(out, "allowBlank", v.allow_blank)?;
        flag(out, "showDropDown", v.show_drop_down)?;
        flag(out, "showInputMessage", v.show_input_message)?;
        flag(out, "showErrorMessage", v.show_error_message)?;
        text_attr(out, "errorTitle", &v.error_title)?;
        text_attr(out, "error", &v.error)?;
        text_attr(out, "promptTitle", &v.prompt_title)?;
        text_attr(out, "prompt", &v.prompt)?;
        out.write_all(b">")?;
        formula(out, "formula1", &v.formula1, uri)?;
        formula(out, "formula2", &v.formula2, uri)?;
        out.write_all(b"</dataValidation>")?;
    }
    out.write_all(b"</dataValidations>")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::xml::{MAIN_URI, STRICT_MAIN_URI};
    use crabxl_core::ResourceLimits;
    use std::io::Cursor;

    const SAMPLE: &str = r#"<dataValidations count="7"><dataValidation sqref="A1:B1 Z1" showDropDown="0" showInputMessage="1" showErrorMessage="1" allowBlank="1" errorTitle="T&amp;" error="Bad &lt;x&gt;" promptTitle="p" prompt="pp" type="whole" errorStyle="stop" operator="between"><formula1>1</formula1><formula2>10</formula2></dataValidation><dataValidation sqref="A2:B2 Z2" showDropDown="0" showInputMessage="1" showErrorMessage="1" allowBlank="1" errorTitle="T&amp;" error="Bad &lt;x&gt;" promptTitle="p" prompt="pp" type="decimal" errorStyle="warning" operator="greaterThan"><formula1>0.5</formula1></dataValidation><dataValidation sqref="A3:B3 Z3" showDropDown="0" showInputMessage="1" showErrorMessage="1" allowBlank="1" errorTitle="T&amp;" error="Bad &lt;x&gt;" promptTitle="p" prompt="pp" type="list" errorStyle="information"><formula1>"a,b,c"</formula1></dataValidation><dataValidation sqref="A4:B4 Z4" showDropDown="0" showInputMessage="1" showErrorMessage="1" allowBlank="1" errorTitle="T&amp;" error="Bad &lt;x&gt;" promptTitle="p" prompt="pp" type="date" errorStyle="stop" operator="lessThan"><formula1>43000</formula1></dataValidation><dataValidation sqref="A5:B5 Z5" showDropDown="0" showInputMessage="1" showErrorMessage="1" allowBlank="1" errorTitle="T&amp;" error="Bad &lt;x&gt;" promptTitle="p" prompt="pp" type="time" errorStyle="warning" operator="notEqual"><formula1>0.5</formula1></dataValidation><dataValidation sqref="A6:B6 Z6" showDropDown="0" showInputMessage="1" showErrorMessage="1" allowBlank="1" errorTitle="T&amp;" error="Bad &lt;x&gt;" promptTitle="p" prompt="pp" type="textLength" errorStyle="information" operator="lessThanOrEqual"><formula1>5</formula1></dataValidation><dataValidation sqref="A7:B7 Z7" showDropDown="0" showInputMessage="1" showErrorMessage="1" allowBlank="1" errorTitle="T&amp;" error="Bad &lt;x&gt;" promptTitle="p" prompt="pp" type="custom" errorStyle="stop"><formula1>A1&gt;1</formula1></dataValidation></dataValidations>"#;

    fn parse(doc: &str) -> Result<DataValidations> {
        let mut xml = XmlStream::new(
            Cursor::new(doc.as_bytes().to_vec()),
            "t".into(),
            doc.len() as u64,
            ResourceLimits::default(),
        );
        loop {
            let frame = xml.next()?;
            let depth = frame.depth;
            match frame.event {
                Event::Start(e) if e.local_name().as_ref().as_bytes() == b"dataValidations" => {
                    let e = e.into_owned();
                    return read(&mut xml, &e, depth);
                }
                Event::Eof => return Err(super::super::invalid("missing")),
                _ => {}
            }
        }
    }
    fn wrap(body: &str, ns: &str) -> String {
        format!("<worksheet xmlns=\"{ns}\">{body}</worksheet>")
    }

    #[test]
    fn openpyxl_sample_round_trips() {
        let parsed = parse(&wrap(SAMPLE, MAIN_URI)).unwrap();
        assert_eq!(parsed.validations.len(), 7);
        let mut out = Vec::new();
        write(&mut out, &parsed, None).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(parse(&wrap(&text, MAIN_URI)).unwrap(), parsed);
    }
    #[test]
    fn fields_are_decoded() {
        let p = parse(&wrap(SAMPLE, MAIN_URI)).unwrap();
        let v = &p.validations[0];
        assert_eq!(v.ranges.len(), 2);
        assert_eq!(v.validation_type, Some(ValidationType::Whole));
        assert_eq!(v.operator, Some(ValidationOperator::Between));
        assert_eq!(v.error_style, Some(ValidationErrorStyle::Stop));
        assert_eq!(v.show_drop_down, Some(false));
        assert_eq!(v.error.as_deref(), Some("Bad <x>"));
        assert_eq!(v.formula2.as_deref(), Some("10"));
        assert_eq!(p.validations[2].formula1.as_deref(), Some("\"a,b,c\""));
        assert_eq!(p.validations[6].formula1.as_deref(), Some("A1>1"));
    }
    #[test]
    fn container_attributes_and_ime_round_trip() {
        let doc = wrap(
            "<dataValidations count=\"1\" disablePrompts=\"1\" xWindow=\"3\" yWindow=\"4\"><dataValidation sqref=\"A1\" imeMode=\"fullKatakana\" type=\"list\"><formula1>\"x\"</formula1></dataValidation></dataValidations>",
            MAIN_URI,
        );
        let p = parse(&doc).unwrap();
        assert_eq!(p.disable_prompts, Some(true));
        assert_eq!((p.x_window, p.y_window), (Some(3), Some(4)));
        assert_eq!(p.validations[0].ime_mode, Some(ImeMode::FullKatakana));
        let mut out = Vec::new();
        write(&mut out, &p, None).unwrap();
        assert_eq!(
            parse(&wrap(std::str::from_utf8(&out).unwrap(), MAIN_URI)).unwrap(),
            p
        );
    }
    #[test]
    fn strict_namespace_round_trips() {
        let parsed = parse(&wrap(SAMPLE, STRICT_MAIN_URI)).unwrap();
        let mut out = Vec::new();
        write(&mut out, &parsed, Some(STRICT_MAIN_URI)).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(STRICT_MAIN_URI));
        assert_eq!(
            parse(&format!("<worksheet>{text}</worksheet>")).unwrap(),
            parsed
        );
    }
    #[test]
    fn unknown_items_are_unsupported() {
        for body in [
            "<dataValidations><bogus/></dataValidations>",
            "<dataValidations bad=\"1\"/>",
            "<dataValidations><dataValidation sqref=\"A1\" bad=\"1\"/></dataValidations>",
            "<dataValidations><dataValidation sqref=\"A1\"><formula3/></dataValidation></dataValidations>",
            "<dataValidations><dataValidation sqref=\"A1\" type=\"nope\"/></dataValidations>",
        ] {
            let error = parse(&wrap(body, MAIN_URI)).unwrap_err();
            assert_eq!(error.kind(), crabxl_core::ErrorKind::Unsupported, "{body}");
        }
    }
    #[test]
    fn empty_writes_nothing() {
        let mut out = Vec::new();
        write(&mut out, &DataValidations::default(), None).unwrap();
        assert!(out.is_empty());
    }
}
