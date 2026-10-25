// SPDX-License-Identifier: MIT
// Attribute set follows openpyxl 3.1.5 (MIT) worksheet/workbook protection.
//! `<sheetProtection>` and `<workbookProtection>` codecs.
use crate::encode::write_attribute;
use crate::printing::namespace;
use crate::xml::XmlStream;
use crabxl_core::Result;
use crabxl_core::features::{SheetProtection, WorkbookProtection};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(super::invalid("Invalid protection boolean")),
    }
}
fn parse_u32(value: &str) -> Result<u32> {
    value
        .trim()
        .parse()
        .map_err(|_| super::invalid("Invalid protection integer"))
}
fn text(value: &str) -> Box<str> {
    value.into()
}

/// Visit every attribute except namespace declarations and foreign-prefixed ones.
fn each_attribute(
    start: &BytesStart<'_>,
    mut visit: impl FnMut(&str, &str) -> Result<bool>,
    what: &str,
) -> Result<()> {
    for attr in start.attributes().with_checks(false) {
        let attr = attr.map_err(|e| {
            crabxl_core::Error::caused_by(crabxl_core::ErrorKind::Xml, "Invalid XML attribute", e)
        })?;
        let key = String::from_utf8_lossy(attr.key.as_ref().as_bytes()).into_owned();
        if key == "xmlns" || key.starts_with("xmlns:") || key.contains(':') {
            continue;
        }
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| {
                crabxl_core::Error::caused_by(
                    crabxl_core::ErrorKind::Xml,
                    "Invalid XML attribute value",
                    e,
                )
            })?;
        if !visit(&key, &value)? {
            return Err(super::unsupported(&format!(
                "Unsupported {what} attribute {key}"
            )));
        }
    }
    Ok(())
}

/// Consume the (childless) element through its matching End.
fn finish<B: BufRead>(xml: &mut XmlStream<B>, depth: usize, what: &str) -> Result<()> {
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(_) => {
                return Err(super::unsupported(&format!(
                    "Unsupported {what} child element"
                )));
            }
            Event::End(_) if frame.depth + 1 == depth => return Ok(()),
            Event::Eof => return Err(super::invalid("Incomplete protection element")),
            _ => {}
        }
    }
}

pub(crate) fn read_sheet<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<SheetProtection> {
    let mut p = SheetProtection::default();
    each_attribute(
        start,
        |key, value| {
            match key {
                "sheet" => p.sheet = parse_bool(value)?,
                "objects" => p.objects = parse_bool(value)?,
                "scenarios" => p.scenarios = parse_bool(value)?,
                "formatCells" => p.format_cells = parse_bool(value)?,
                "formatColumns" => p.format_columns = parse_bool(value)?,
                "formatRows" => p.format_rows = parse_bool(value)?,
                "insertColumns" => p.insert_columns = parse_bool(value)?,
                "insertRows" => p.insert_rows = parse_bool(value)?,
                "insertHyperlinks" => p.insert_hyperlinks = parse_bool(value)?,
                "deleteColumns" => p.delete_columns = parse_bool(value)?,
                "deleteRows" => p.delete_rows = parse_bool(value)?,
                "selectLockedCells" => p.select_locked_cells = parse_bool(value)?,
                "selectUnlockedCells" => p.select_unlocked_cells = parse_bool(value)?,
                "sort" => p.sort = parse_bool(value)?,
                "autoFilter" => p.auto_filter = parse_bool(value)?,
                "pivotTables" => p.pivot_tables = parse_bool(value)?,
                "password" => p.password = Some(text(value)),
                "algorithmName" => p.algorithm_name = Some(text(value)),
                "hashValue" => p.hash_value = Some(text(value)),
                "saltValue" => p.salt_value = Some(text(value)),
                "spinCount" => p.spin_count = Some(parse_u32(value)?),
                _ => return Ok(false),
            }
            Ok(true)
        },
        "sheetProtection",
    )?;
    finish(xml, depth, "sheetProtection")?;
    Ok(p)
}

fn flag(out: &mut impl Write, name: &str, value: bool) -> io::Result<()> {
    write_attribute(out, name, if value { "1" } else { "0" })
}

pub(crate) fn write_sheet(
    out: &mut impl Write,
    p: &SheetProtection,
    uri: Option<&str>,
) -> io::Result<()> {
    out.write_all(b"<sheetProtection")?;
    namespace(out, uri)?;
    if let Some(v) = &p.password {
        write_attribute(out, "password", v)?;
    }
    if let Some(v) = &p.algorithm_name {
        write_attribute(out, "algorithmName", v)?;
    }
    if let Some(v) = &p.hash_value {
        write_attribute(out, "hashValue", v)?;
    }
    if let Some(v) = &p.salt_value {
        write_attribute(out, "saltValue", v)?;
    }
    if let Some(v) = p.spin_count {
        write_attribute(out, "spinCount", &v.to_string())?;
    }
    for (name, value) in [
        ("sheet", p.sheet),
        ("objects", p.objects),
        ("scenarios", p.scenarios),
        ("formatCells", p.format_cells),
        ("formatColumns", p.format_columns),
        ("formatRows", p.format_rows),
        ("insertColumns", p.insert_columns),
        ("insertRows", p.insert_rows),
        ("insertHyperlinks", p.insert_hyperlinks),
        ("deleteColumns", p.delete_columns),
        ("deleteRows", p.delete_rows),
        ("selectLockedCells", p.select_locked_cells),
        ("selectUnlockedCells", p.select_unlocked_cells),
        ("sort", p.sort),
        ("autoFilter", p.auto_filter),
        ("pivotTables", p.pivot_tables),
    ] {
        flag(out, name, value)?;
    }
    out.write_all(b"/>")
}

pub(crate) fn read_workbook<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<WorkbookProtection> {
    let mut p = WorkbookProtection::default();
    each_attribute(
        start,
        |key, value| {
            match key {
                "workbookPassword" => p.workbook_password = Some(text(value)),
                "workbookPasswordCharacterSet" => {
                    p.workbook_password_character_set = Some(text(value));
                }
                "revisionsPassword" => p.revisions_password = Some(text(value)),
                "revisionsPasswordCharacterSet" => {
                    p.revisions_password_character_set = Some(text(value));
                }
                "lockStructure" => p.lock_structure = Some(parse_bool(value)?),
                "lockWindows" => p.lock_windows = Some(parse_bool(value)?),
                "lockRevision" => p.lock_revision = Some(parse_bool(value)?),
                "revisionsAlgorithmName" => p.revisions_algorithm_name = Some(text(value)),
                "revisionsHashValue" => p.revisions_hash_value = Some(text(value)),
                "revisionsSaltValue" => p.revisions_salt_value = Some(text(value)),
                "revisionsSpinCount" => p.revisions_spin_count = Some(parse_u32(value)?),
                "workbookAlgorithmName" => p.workbook_algorithm_name = Some(text(value)),
                "workbookHashValue" => p.workbook_hash_value = Some(text(value)),
                "workbookSaltValue" => p.workbook_salt_value = Some(text(value)),
                "workbookSpinCount" => p.workbook_spin_count = Some(parse_u32(value)?),
                _ => return Ok(false),
            }
            Ok(true)
        },
        "workbookProtection",
    )?;
    finish(xml, depth, "workbookProtection")?;
    Ok(p)
}

pub(crate) fn write_workbook(
    out: &mut impl Write,
    p: &WorkbookProtection,
    uri: Option<&str>,
) -> io::Result<()> {
    out.write_all(b"<workbookProtection")?;
    namespace(out, uri)?;
    for (name, value) in [
        ("workbookPassword", &p.workbook_password),
        (
            "workbookPasswordCharacterSet",
            &p.workbook_password_character_set,
        ),
        ("revisionsPassword", &p.revisions_password),
        (
            "revisionsPasswordCharacterSet",
            &p.revisions_password_character_set,
        ),
    ] {
        if let Some(v) = value {
            write_attribute(out, name, v)?;
        }
    }
    for (name, value) in [
        ("lockStructure", p.lock_structure),
        ("lockWindows", p.lock_windows),
        ("lockRevision", p.lock_revision),
    ] {
        if let Some(v) = value {
            flag(out, name, v)?;
        }
    }
    for (name, value) in [
        ("revisionsAlgorithmName", &p.revisions_algorithm_name),
        ("revisionsHashValue", &p.revisions_hash_value),
        ("revisionsSaltValue", &p.revisions_salt_value),
    ] {
        if let Some(v) = value {
            write_attribute(out, name, v)?;
        }
    }
    if let Some(v) = p.revisions_spin_count {
        write_attribute(out, "revisionsSpinCount", &v.to_string())?;
    }
    for (name, value) in [
        ("workbookAlgorithmName", &p.workbook_algorithm_name),
        ("workbookHashValue", &p.workbook_hash_value),
        ("workbookSaltValue", &p.workbook_salt_value),
    ] {
        if let Some(v) = value {
            write_attribute(out, name, v)?;
        }
    }
    if let Some(v) = p.workbook_spin_count {
        write_attribute(out, "workbookSpinCount", &v.to_string())?;
    }
    out.write_all(b"/>")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::xml::Scope;
    use crabxl_core::{ErrorKind, ResourceLimits};
    use std::io::Cursor;

    fn stream(xml: &str) -> XmlStream<Cursor<Vec<u8>>> {
        XmlStream::new(
            Cursor::new(xml.as_bytes().to_vec()),
            "t.xml".into(),
            1 << 20,
            ResourceLimits::default(),
        )
    }
    /// Read the root element of `xml` with `read`.
    fn root<T>(
        xml: &str,
        read: impl Fn(&mut XmlStream<Cursor<Vec<u8>>>, &BytesStart<'_>, usize) -> Result<T>,
    ) -> Result<T> {
        let mut s = stream(xml);
        loop {
            let (start, depth, scope) = {
                let frame = s.next()?;
                match &frame.event {
                    Event::Start(e) => (e.to_owned(), frame.depth, frame.scope),
                    Event::Eof => return Err(super::super::invalid("no root")),
                    _ => continue,
                }
            };
            assert!(scope == Scope::Spreadsheet);
            return read(&mut s, &start, depth);
        }
    }
    fn sheet(xml: &str) -> Result<SheetProtection> {
        root(xml, read_sheet)
    }
    fn book(xml: &str) -> Result<WorkbookProtection> {
        root(xml, read_workbook)
    }
    const SHEET: &str = r#"<sheetProtection selectLockedCells="0" selectUnlockedCells="0" sheet="1" objects="1" insertRows="1" insertHyperlinks="1" autoFilter="1" scenarios="0" formatColumns="1" deleteColumns="1" insertColumns="1" pivotTables="1" deleteRows="1" formatCells="0" formatRows="1" sort="1" password="DAA7"/>"#;
    const BOOK: &str = r#"<workbookProtection workbookPassword="CF75" revisionsPassword="CF75" lockStructure="1" lockWindows="0" lockRevision="1" workbookAlgorithmName="SHA-512" workbookHashValue="aGFzaA==" workbookSaltValue="c2FsdA==" workbookSpinCount="100000"/>"#;

    #[test]
    fn sheet_openpyxl_roundtrip() {
        let p = sheet(&format!(
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{SHEET}</worksheet>"#
        ));
        // Root here is worksheet, so read the child directly instead.
        assert!(p.is_err());
        let p = sheet(&SHEET.replace(
            "<sheetProtection",
            "<sheetProtection xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"",
        ))
        .unwrap();
        assert!(p.sheet && p.objects && !p.format_cells && p.sort);
        assert_eq!(p.password.as_deref(), Some("DAA7"));
        let mut out = Vec::new();
        write_sheet(&mut out, &p, Some(crate::xml::MAIN_URI)).unwrap();
        let again = sheet(std::str::from_utf8(&out).unwrap()).unwrap();
        assert_eq!(p, again);
    }
    #[test]
    fn sheet_modern_hash_and_strict() {
        let xml = r#"<sheetProtection xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main" sheet="true" algorithmName="SHA-512" hashValue="aA==" saltValue="cw==" spinCount="100000"/>"#;
        let p = sheet(xml).unwrap();
        assert_eq!(p.spin_count, Some(100000));
        let mut out = Vec::new();
        write_sheet(&mut out, &p, Some(crate::xml::STRICT_MAIN_URI)).unwrap();
        assert_eq!(sheet(std::str::from_utf8(&out).unwrap()).unwrap(), p);
    }
    #[test]
    fn workbook_openpyxl_roundtrip() {
        let p = book(&BOOK.replace(
            "<workbookProtection",
            "<workbookProtection xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"",
        ))
        .unwrap();
        assert_eq!(p.lock_structure, Some(true));
        assert_eq!(p.lock_windows, Some(false));
        assert_eq!(p.workbook_spin_count, Some(100000));
        assert_eq!(p.workbook_password.as_deref(), Some("CF75"));
        let mut out = Vec::new();
        write_workbook(&mut out, &p, Some(crate::xml::MAIN_URI)).unwrap();
        assert_eq!(book(std::str::from_utf8(&out).unwrap()).unwrap(), p);
    }
    #[test]
    fn rejects_unknown() {
        let ns = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"";
        let e = sheet(&format!("<sheetProtection {ns} bogus=\"1\"/>")).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let e = sheet(&format!("<sheetProtection {ns}><x/></sheetProtection>")).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let e = book(&format!("<workbookProtection {ns} bogus=\"1\"/>")).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let e = book(&format!("<workbookProtection {ns} lockStructure=\"x\"/>")).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InvalidData);
    }
}
