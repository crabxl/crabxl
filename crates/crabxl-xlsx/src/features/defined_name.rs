//! Workbook `<definedNames>` codec.
use super::{invalid, unsupported};
use crate::encode::{write_attribute, write_text};
use crate::xml::{Scope, XmlStream, append_xml_text};
use crabxl_core::Result;
use crabxl_core::features::DefinedName;
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

const MAX_VALUE: usize = 1 << 20;

fn boolean(value: &str) -> Result<bool> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(invalid("Invalid defined name boolean")),
    }
}
fn integer(value: &str) -> Result<u32> {
    value
        .parse()
        .map_err(|_| invalid("Invalid defined name integer"))
}

fn read_name(start: &BytesStart<'_>) -> Result<DefinedName> {
    let mut name = DefinedName::default();
    let mut has_name = false;
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|e| {
            crabxl_core::Error::caused_by(crabxl_core::ErrorKind::Xml, "Invalid XML attribute", e)
        })?;
        let full = attribute.key.as_ref();
        if full == "xmlns" || full.starts_with("xmlns:") {
            continue;
        }
        let key = attribute.key.local_name();
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| {
                crabxl_core::Error::caused_by(
                    crabxl_core::ErrorKind::Xml,
                    "Invalid XML attribute value",
                    e,
                )
            })?;
        let text: Box<str> = value.as_ref().into();
        match key.as_ref().as_bytes() {
            b"name" => {
                name.name = text;
                has_name = true;
            }
            b"comment" => name.comment = Some(text),
            b"customMenu" => name.custom_menu = Some(text),
            b"description" => name.description = Some(text),
            b"help" => name.help = Some(text),
            b"statusBar" => name.status_bar = Some(text),
            b"localSheetId" => name.local_sheet_id = Some(integer(&text)?),
            b"hidden" => name.hidden = Some(boolean(&text)?),
            b"function" => name.function = Some(boolean(&text)?),
            b"vbProcedure" => name.vb_procedure = Some(boolean(&text)?),
            b"xlm" => name.xlm = Some(boolean(&text)?),
            b"functionGroupId" => name.function_group_id = Some(integer(&text)?),
            b"shortcutKey" => name.shortcut_key = Some(text),
            b"publishToServer" => name.publish_to_server = Some(boolean(&text)?),
            b"workbookParameter" => name.workbook_parameter = Some(boolean(&text)?),
            _ => return Err(unsupported("Unknown definedName attribute")),
        }
    }
    if !has_name {
        return Err(invalid("definedName without name"));
    }
    Ok(name)
}

/// Read the container. `start` is the consumed `<definedNames>` start at `depth`.
pub(crate) fn read<B: BufRead>(
    xml: &mut XmlStream<B>,
    _start: &BytesStart<'_>,
    depth: usize,
) -> Result<Vec<DefinedName>> {
    let mut names = Vec::new();
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == depth + 1 => {
                if frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"definedName"
                {
                    return Err(unsupported("Unknown definedNames child"));
                }
                let mut name = read_name(&e)?;
                let mut value = String::new();
                loop {
                    let inner = xml.next()?;
                    match &inner.event {
                        Event::Start(_) => return Err(unsupported("Unknown definedName child")),
                        Event::End(_) if inner.depth + 1 == depth + 1 => break,
                        Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                            append_xml_text(&mut value, &inner.event, MAX_VALUE)?
                        }
                        Event::Eof => return Err(invalid("Incomplete definedName")),
                        _ => {}
                    }
                }
                name.value = value.into();
                names.push(name);
            }
            Event::Start(_) => return Err(unsupported("Unknown definedNames child")),
            Event::End(_) if frame.depth + 1 == depth => return Ok(names),
            Event::Eof => return Err(invalid("Incomplete definedNames")),
            _ => {}
        }
    }
}

/// Write the container; writes nothing for an empty slice.
pub(crate) fn write(
    out: &mut impl Write,
    names: &[DefinedName],
    uri: Option<&str>,
) -> io::Result<()> {
    if names.is_empty() {
        return Ok(());
    }
    out.write_all(b"<definedNames")?;
    crate::printing::namespace(out, uri)?;
    out.write_all(b">")?;
    for name in names {
        out.write_all(b"<definedName")?;
        write_attribute(out, "name", &name.name)?;
        for (key, value) in [
            ("comment", &name.comment),
            ("customMenu", &name.custom_menu),
            ("description", &name.description),
            ("help", &name.help),
            ("statusBar", &name.status_bar),
        ] {
            if let Some(value) = value {
                write_attribute(out, key, value)?;
            }
        }
        if let Some(id) = name.local_sheet_id {
            write_attribute(out, "localSheetId", &id.to_string())?;
        }
        for (key, value) in [
            ("hidden", name.hidden),
            ("function", name.function),
            ("vbProcedure", name.vb_procedure),
            ("xlm", name.xlm),
        ] {
            if let Some(value) = value {
                write_attribute(out, key, if value { "1" } else { "0" })?;
            }
        }
        if let Some(id) = name.function_group_id {
            write_attribute(out, "functionGroupId", &id.to_string())?;
        }
        if let Some(value) = &name.shortcut_key {
            write_attribute(out, "shortcutKey", value)?;
        }
        for (key, value) in [
            ("publishToServer", name.publish_to_server),
            ("workbookParameter", name.workbook_parameter),
        ] {
            if let Some(value) = value {
                write_attribute(out, key, if value { "1" } else { "0" })?;
            }
        }
        out.write_all(b">")?;
        write_text(out, &name.value)?;
        out.write_all(b"</definedName>")?;
    }
    out.write_all(b"</definedNames>")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crabxl_core::{ErrorKind, ResourceLimits};
    use std::io::Cursor;

    fn parse(xml: &str) -> Result<Vec<DefinedName>> {
        let xml = if xml.contains("xmlns") {
            xml.to_owned()
        } else {
            xml.replacen(
                "<definedNames",
                "<definedNames xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"",
                1,
            )
        };
        let mut stream = XmlStream::new(
            Cursor::new(xml.into_bytes()),
            "t".into(),
            1 << 20,
            ResourceLimits::default(),
        );
        loop {
            let frame = stream.next()?;
            let depth = frame.depth;
            if let Event::Start(e) = frame.event {
                let e = e.into_owned();
                return read(&mut stream, &e, depth);
            }
        }
    }

    const OPENPYXL: &str = r#"<definedNames><definedName name="glob">'My Sheet''s'!$A$1:$B$2</definedName><definedName name="_xlnm.Print_Titles" localSheetId="0">'My Sheet''s'!$1:$2,'My Sheet''s'!$A:$B</definedName><definedName name="_xlnm.Print_Area" localSheetId="0">'My Sheet''s'!$A$1:$D$4,'My Sheet''s'!$F$6:$G$8</definedName><definedName name="loc" comment="c&lt;&amp;&gt;" localSheetId="1" hidden="1">B!$C$3</definedName><definedName name="_xlnm.Print_Titles" localSheetId="1">'B'!$3:$3</definedName></definedNames>"#;

    #[test]
    fn openpyxl_round_trip() {
        let names = parse(OPENPYXL).unwrap();
        assert_eq!(names.len(), 5);
        assert_eq!(names[0].local_sheet_id, None);
        assert_eq!(&*names[3].value, "B!$C$3");
        assert_eq!(names[3].comment.as_deref(), Some("c<&>"));
        assert_eq!(names[3].hidden, Some(true));
        assert_eq!(names[1].print_title_rows(), Some((1, 2)));
        assert_eq!(names[2].print_area_ranges().len(), 2);
        let mut out = Vec::new();
        write(&mut out, &names, None).unwrap();
        assert_eq!(String::from_utf8(out.clone()).unwrap(), OPENPYXL);
        assert_eq!(parse(std::str::from_utf8(&out).unwrap()).unwrap(), names);
    }

    #[test]
    fn helpers_match_openpyxl_text() {
        let area = DefinedName::print_area(0, "My Sheet's", &["F6:G8", "A1:D4"]);
        let titles = DefinedName::print_titles(0, "My Sheet's", Some((1, 2)), Some((1, 2)));
        let parsed = parse(OPENPYXL).unwrap();
        assert_eq!(area, parsed[2]);
        assert_eq!(titles, parsed[1]);
    }

    #[test]
    fn strict_namespace_and_all_attributes() {
        let xml = r#"<definedNames xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main"><definedName name="n" comment="a" customMenu="b" description="c" help="d" statusBar="e" localSheetId="2" hidden="1" function="0" vbProcedure="1" xlm="0" functionGroupId="4" shortcutKey="k" publishToServer="1" workbookParameter="0">1+1</definedName></definedNames>"#;
        let names = parse(xml).unwrap();
        let n = &names[0];
        assert_eq!(n.function_group_id, Some(4));
        assert_eq!(n.workbook_parameter, Some(false));
        assert_eq!(n.shortcut_key.as_deref(), Some("k"));
        let mut out = Vec::new();
        write(
            &mut out,
            &names,
            Some("http://purl.oclc.org/ooxml/spreadsheetml/main"),
        )
        .unwrap();
        assert_eq!(parse(std::str::from_utf8(&out).unwrap()).unwrap(), names);
        let mut empty = Vec::new();
        write(&mut empty, &[], None).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn unknown_items_are_rejected() {
        let e = parse("<definedNames><x/></definedNames>").unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let e =
            parse(r#"<definedNames><definedName name="a" foo="1">x</definedName></definedNames>"#)
                .unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let e = parse(r#"<definedNames><definedName name="a"><b/></definedName></definedNames>"#)
            .unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
    }
}
