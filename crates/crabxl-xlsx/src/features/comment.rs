// SPDX-License-Identifier: MIT
// Part layout and VML shapes follow openpyxl 3.1.5 (MIT) comments/comment_sheet.py and shape_writer.py.
//! Comments part (`xl/commentsN.xml`) and legacy VML drawing codecs.
use crate::encode::{write_attribute, write_text};
use crate::xml::{Scope, XmlStream, append_xml_text, attribute};
use crabxl_core::features::Comment;
use crabxl_core::{CellAddress, Result, RichText};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

/// Upper bound for a single comment text (Excel allows 32,767 characters).
const MAX_TEXT_BYTES: usize = 1 << 20;

fn name_of(e: &quick_xml::events::BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into_owned()
}
fn read_author<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<Box<str>> {
    let mut text = String::new();
    loop {
        let frame = xml.next()?;
        match &frame.event {
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                append_xml_text(&mut text, &frame.event, MAX_TEXT_BYTES)?;
            }
            Event::End(_) if frame.depth + 1 == depth => return Ok(text.into()),
            Event::Start(_) => return Err(super::unsupported("Unknown element in author")),
            Event::Eof => return Err(super::invalid("Incomplete author element")),
            _ => {}
        }
    }
}

struct Raw {
    comment: Comment,
    author_id: usize,
}

fn read_comment<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &quick_xml::events::BytesStart<'_>,
    depth: usize,
) -> Result<Raw> {
    let mut reference = None;
    let mut author_id = None;
    for attr in start.attributes().with_checks(false) {
        let attr = attr.map_err(|e| {
            crabxl_core::Error::caused_by(crabxl_core::ErrorKind::Xml, "Invalid XML attribute", e)
        })?;
        let key = String::from_utf8_lossy(attr.key.as_ref().as_bytes()).into_owned();
        if key.contains(':') || matches!(key.as_str(), "guid" | "shapeId") {
            continue;
        }
        let value = || -> Result<String> {
            attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|v| v.into_owned())
                .map_err(|e| {
                    crabxl_core::Error::caused_by(
                        crabxl_core::ErrorKind::Xml,
                        "Invalid XML attribute value",
                        e,
                    )
                })
        };
        match key.as_str() {
            "ref" => reference = Some(value()?),
            "authorId" => {
                author_id = Some(
                    value()?
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| super::invalid("Invalid comment authorId"))?,
                );
            }
            _ => return Err(super::unsupported("Unsupported comment attribute")),
        }
    }
    let reference = reference.ok_or_else(|| super::invalid("Missing comment ref"))?;
    let address: CellAddress = reference.parse()?;
    let author_id = author_id.ok_or_else(|| super::invalid("Missing comment authorId"))?;
    let mut comment = Comment {
        address,
        ..Comment::default()
    };
    let mut seen_text = false;
    loop {
        let frame = xml.next()?;
        match &frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet && frame.depth == depth + 1 => {
                if name_of(e) != "text" || seen_text {
                    return Err(super::unsupported("Unsupported comment child element"));
                }
                seen_text = true;
                let parsed = crate::rich_text::read_container(
                    xml,
                    depth + 1,
                    b"text",
                    MAX_TEXT_BYTES,
                    true,
                )?;
                match parsed {
                    crate::rich_text::ParsedText::Plain(text) => comment.text = text,
                    crate::rich_text::ParsedText::Rich(rich) => {
                        comment.text = rich.plain_text()?;
                        comment.rich_text = Some(*rich);
                    }
                }
            }
            Event::Start(_) => return Err(super::unsupported("Unsupported comment child element")),
            Event::End(_) if frame.depth + 1 == depth => break,
            Event::Eof => return Err(super::invalid("Incomplete comment element")),
            _ => {}
        }
    }
    Ok(Raw { comment, author_id })
}

/// Read a whole comments part.
pub(crate) fn read_part<B: BufRead>(xml: &mut XmlStream<B>) -> Result<Vec<Comment>> {
    let mut authors: Vec<Box<str>> = Vec::new();
    let mut raws: Vec<Raw> = Vec::new();
    let mut in_authors = false;
    let mut in_list = false;
    loop {
        let frame = xml.next()?;
        crate::xml::check_declaration(&frame.event)?;
        match &frame.event {
            Event::Start(e) if frame.scope != Scope::Spreadsheet => {
                let _ = e;
                let depth = frame.depth;
                crate::style_codec::skip(xml, depth)?;
            }
            Event::Start(e) => {
                let depth = frame.depth;
                let name = name_of(e);
                match (depth, name.as_str()) {
                    (1, "comments") => {}
                    (1, _) => return Err(super::invalid("Expected comments root")),
                    (2, "authors") => in_authors = true,
                    (2, "commentList") => in_list = true,
                    (2, "extLst") => crate::style_codec::skip(xml, depth)?,
                    (3, "author") if in_authors => authors.push(read_author(xml, depth)?),
                    (3, "comment") if in_list => {
                        let e = e.to_owned();
                        raws.push(read_comment(xml, &e, depth)?);
                    }
                    _ => return Err(super::unsupported("Unsupported comments element")),
                }
            }
            Event::End(_) if frame.depth == 1 => {
                in_authors = false;
                in_list = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    raws.into_iter()
        .map(|raw| {
            let mut comment = raw.comment;
            comment.author = authors
                .get(raw.author_id)
                .cloned()
                .ok_or_else(|| super::invalid("Comment authorId out of range"))?;
            Ok(comment)
        })
        .collect()
}

fn pixels(value: &str) -> Option<f64> {
    let value = value.trim();
    let (number, factor) = if let Some(v) = value.strip_suffix("px") {
        (v, 1.0)
    } else if let Some(v) = value.strip_suffix("pt") {
        (v, 4.0 / 3.0)
    } else if let Some(v) = value.strip_suffix("in") {
        (v, 96.0)
    } else {
        (value, 1.0)
    };
    let parsed = number.trim().parse::<f64>().ok()? * factor;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

struct Shape {
    style: Option<String>,
    row: Option<u32>,
    column: Option<u32>,
    visible: bool,
}

/// Apply shape data (size, visibility) from the sheet's VML drawing part.
/// The VML part is not SpreadsheetML; `bytes` is the raw part.
pub(crate) fn apply_vml(bytes: &[u8], comments: &mut [Comment]) -> Result<()> {
    let mut index: HashMap<(u32, u32), usize> = HashMap::with_capacity(comments.len());
    for (i, c) in comments.iter().enumerate() {
        index.insert((c.address.row.get(), c.address.column.get()), i);
    }
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = false;
    let mut buffer = Vec::new();
    let mut shape: Option<Shape> = None;
    let mut field: Option<bool> = None; // Some(true) = Row, Some(false) = Column
    loop {
        buffer.clear();
        let event = match reader.read_event_into(&mut buffer) {
            Ok(event) => event,
            Err(_) => return Ok(()),
        };
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(event, Event::Empty(_));
                match name_of(e).as_str() {
                    "shape" if !is_empty => {
                        shape = Some(Shape {
                            style: attribute(e, b"style").ok().flatten(),
                            row: None,
                            column: None,
                            visible: false,
                        });
                    }
                    "Visible" => {
                        if let Some(s) = shape.as_mut() {
                            s.visible = true;
                        }
                    }
                    "Row" if !is_empty => field = Some(true),
                    "Column" if !is_empty => field = Some(false),
                    _ => {}
                }
            }
            Event::Text(ref t) => {
                if let (Some(is_row), Some(s)) = (field, shape.as_mut()) {
                    let parsed = t.xml10_content().trim().parse().ok();
                    if is_row {
                        s.row = parsed;
                    } else {
                        s.column = parsed;
                    }
                }
            }
            Event::End(ref e) => match name_of_end(e).as_str() {
                "Row" | "Column" => field = None,
                "shape" => {
                    if let Some(s) = shape.take()
                        && let (Some(row), Some(column)) = (s.row, s.column)
                        && let Some(&i) = index.get(&(row, column))
                    {
                        apply_shape(&s, &mut comments[i]);
                    }
                }
                _ => {}
            },
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}
fn name_of_end(e: &quick_xml::events::BytesEnd<'_>) -> String {
    String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into_owned()
}
fn apply_shape(shape: &Shape, comment: &mut Comment) {
    let mut visible = shape.visible;
    if let Some(style) = &shape.style {
        for item in style.split(';') {
            let Some((key, value)) = item.split_once(':') else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "width" => {
                    if let Some(v) = pixels(value) {
                        comment.width = v;
                    }
                }
                "height" => {
                    if let Some(v) = pixels(value) {
                        comment.height = v;
                    }
                }
                "visibility" => visible |= value.trim().eq_ignore_ascii_case("visible"),
                _ => {}
            }
        }
    }
    comment.visible = visible;
}

/// Write a whole comments part.
pub(crate) fn write_part(out: &mut impl Write, comments: &[Comment]) -> io::Result<()> {
    let mut authors: Vec<&str> = Vec::new();
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut author_of = Vec::with_capacity(comments.len());
    for c in comments {
        let next = authors.len();
        let id = *ids.entry(&c.author).or_insert(next);
        if id == next {
            authors.push(&c.author);
        }
        author_of.push(id);
    }
    write!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><comments xmlns=\"{}\"><authors>",
        crate::xml::MAIN_URI
    )?;
    for author in &authors {
        out.write_all(b"<author>")?;
        write_text(out, author)?;
        out.write_all(b"</author>")?;
    }
    out.write_all(b"</authors><commentList>")?;
    for (c, id) in comments.iter().zip(author_of) {
        out.write_all(b"<comment")?;
        write_attribute(out, "ref", &c.address.to_string())?;
        write_attribute(out, "authorId", &id.to_string())?;
        out.write_all(b"><text>")?;
        if let Some(rich) = &c.rich_text {
            write_rich(out, rich)?;
        } else {
            if c.text.trim() == &*c.text && !c.text.contains(['\n', '\r', '\t']) {
                out.write_all(b"<t>")?;
            } else {
                out.write_all(b"<t xml:space=\"preserve\">")?;
            }
            write_text(out, &c.text)?;
            out.write_all(b"</t>")?;
        }
        out.write_all(b"</text></comment>")?;
    }
    out.write_all(b"</commentList></comments>")
}
fn write_rich(out: &mut impl Write, rich: &RichText) -> io::Result<()> {
    let mut buffer = Vec::new();
    crate::rich_text::write_container(&mut buffer, rich)?;
    // Strip the `<is>` / `</is>` wrapper; runs are the same inside `<text>`.
    let inner = buffer
        .strip_prefix(b"<is>")
        .and_then(|b| b.strip_suffix(b"</is>"))
        .ok_or_else(|| io::Error::other("Unexpected rich-text container"))?;
    out.write_all(inner)
}

fn px(value: f64, default: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        default
    }
}

/// Write the VML drawing part for the comments. `sheet_index` is zero-based
/// and keeps shape ids unique across sheets.
pub(crate) fn write_vml(
    out: &mut impl Write,
    comments: &[Comment],
    sheet_index: usize,
) -> io::Result<()> {
    let block = sheet_index.saturating_add(1);
    write!(
        out,
        "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\
<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{block}\"/></o:shapelayout>\
<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\">\
<v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>"
    )?;
    for (i, c) in comments.iter().enumerate() {
        let id = block
            .saturating_mul(1024)
            .saturating_add(2)
            .saturating_add(i);
        write!(
            out,
            "<v:shape id=\"_x0000_s{id:04}\" type=\"#_x0000_t202\" style=\"position:absolute; margin-left:59.25pt;margin-top:1.5pt;width:{}px;height:{}px;z-index:1;visibility:{}\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\">\
<v:fill color2=\"#ffffe1\"/><v:shadow color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/>\
<v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"/></v:textbox>\
<x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:AutoFill>False</x:AutoFill><x:Row>{}</x:Row><x:Column>{}</x:Column>{}</x:ClientData></v:shape>",
            px(c.width, 144.0),
            px(c.height, 79.0),
            if c.visible { "visible" } else { "hidden" },
            c.address.row.get(),
            c.address.column.get(),
            if c.visible { "<x:Visible/>" } else { "" },
        )?;
    }
    out.write_all(b"</xml>")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crabxl_core::{ErrorKind, ResourceLimits};
    use std::io::Cursor;

    fn read(xml: &str) -> Result<Vec<Comment>> {
        let mut s = XmlStream::new(
            Cursor::new(xml.as_bytes().to_vec()),
            "comments.xml".into(),
            1 << 20,
            ResourceLimits::default(),
        );
        read_part(&mut s)
    }
    const OPENPYXL: &str = "<comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors><author>Alice</author><author>Bob</author></authors><commentList><comment ref=\"A1\" authorId=\"0\" shapeId=\"0\"><text><t xml:space=\"preserve\">Hello &amp; &lt;world&gt;\n line2 </t></text></comment><comment ref=\"B2\" authorId=\"0\" shapeId=\"0\"><text><t>Third</t></text></comment><comment ref=\"C5\" authorId=\"1\" shapeId=\"0\"><text><t>Second</t></text></comment></commentList></comments>";
    /// Shape markup exactly as openpyxl 3.1.5 serialises it (ElementTree prefixes).
    fn openpyxl_shape(id: u32, width: u32, height: u32, row: u32, column: u32) -> String {
        format!(
            "<ns0:shape xmlns:ns0=\"urn:schemas-microsoft-com:vml\" xmlns:ns1=\"urn:schemas-microsoft-com:office:office\" type=\"#_x0000_t202\" style=\"position:absolute; margin-left:59.25pt;margin-top:1.5pt;width:{width}px;height:{height}px;z-index:1;visibility:hidden\" fillcolor=\"#ffffe1\" ns1:insetmode=\"auto\" id=\"_x0000_s{id}\"><ns0:fill color2=\"#ffffe1\"/><ns0:shadow color=\"black\" obscured=\"t\"/><ns0:path ns1:connecttype=\"none\"/><ns0:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"/></ns0:textbox><ns2:ClientData xmlns:ns2=\"urn:schemas-microsoft-com:office:excel\" ObjectType=\"Note\"><ns2:MoveWithCells/><ns2:SizeWithCells/><ns2:AutoFill>False</ns2:AutoFill><ns2:Row>{row}</ns2:Row><ns2:Column>{column}</ns2:Column></ns2:ClientData></ns0:shape>"
        )
    }
    fn openpyxl_vml() -> String {
        format!(
            "<xml><ns0:shapelayout xmlns:ns0=\"urn:schemas-microsoft-com:office:office\" xmlns:ns1=\"urn:schemas-microsoft-com:vml\" ns1:ext=\"edit\"><ns0:idmap ns1:ext=\"edit\" data=\"1\"/></ns0:shapelayout>{}{}{}</xml>",
            openpyxl_shape(1026, 200, 100, 0, 0),
            openpyxl_shape(1027, 60, 50, 1, 1),
            openpyxl_shape(1028, 144, 79, 4, 2)
        )
    }

    #[test]
    fn reads_openpyxl_part_and_vml() {
        let mut c = read(OPENPYXL).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(&*c[0].text, "Hello & <world>\n line2 ");
        assert_eq!(&*c[0].author, "Alice");
        assert_eq!(&*c[2].author, "Bob");
        assert_eq!(c[2].address, "C5".parse().unwrap());
        apply_vml(openpyxl_vml().as_bytes(), &mut c).unwrap();
        assert_eq!((c[0].width, c[0].height), (200.0, 100.0));
        assert_eq!((c[1].width, c[1].height), (60.0, 50.0));
        assert_eq!((c[2].width, c[2].height), (144.0, 79.0));
        assert!(c.iter().all(|c| !c.visible));
    }
    #[test]
    fn roundtrip_part_and_vml() {
        let mut c = read(OPENPYXL).unwrap();
        apply_vml(openpyxl_vml().as_bytes(), &mut c).unwrap();
        c[1].visible = true;
        c[1].width = 90.5;
        let mut out = Vec::new();
        write_part(&mut out, &c).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("<?xml"));
        let mut again = read(&text).unwrap();
        let mut vml = Vec::new();
        write_vml(&mut vml, &c, 0).unwrap();
        let vml_text = String::from_utf8(vml.clone()).unwrap();
        assert!(vml_text.contains("_x0000_s1026") && vml_text.contains("_x0000_s1028"));
        apply_vml(&vml, &mut again).unwrap();
        assert_eq!(again, c);
        let mut vml2 = Vec::new();
        write_vml(&mut vml2, &c, 1).unwrap();
        assert!(String::from_utf8(vml2).unwrap().contains("_x0000_s2050"));
    }
    #[test]
    fn rich_runs_roundtrip() {
        let xml = "<comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors><author>A</author></authors><commentList><comment ref=\"D4\" authorId=\"0\"><text><r><rPr><b/><sz val=\"9\"/><rFont val=\"Tahoma\"/></rPr><t>Bold: </t></r><r><t>plain</t></r></text></comment></commentList></comments>";
        let c = read(xml).unwrap();
        assert_eq!(&*c[0].text, "Bold: plain");
        assert_eq!(c[0].rich_text.as_ref().map(|r| r.runs.len()), Some(2));
        let mut out = Vec::new();
        write_part(&mut out, &c).unwrap();
        assert_eq!(read(std::str::from_utf8(&out).unwrap()).unwrap(), c);
    }
    #[test]
    fn strict_namespace_and_authors_dedupe() {
        let xml = "<comments xmlns=\"http://purl.oclc.org/ooxml/spreadsheetml/main\"><authors><author>X</author></authors><commentList><comment ref=\"A1\" authorId=\"0\"><text><t>one</t></text></comment></commentList></comments>";
        let c = read(xml).unwrap();
        assert_eq!(&*c[0].text, "one");
        let many = vec![c[0].clone(), c[0].clone()];
        let mut out = Vec::new();
        write_part(&mut out, &many).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("<author>").count(), 1);
    }
    #[test]
    fn rejects_unknown_and_bad_input() {
        let ns = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"";
        let e = read(&format!("<comments {ns}><bogus/></comments>")).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let e = read(&format!("<comments {ns}><authors><author>A</author></authors><commentList><comment ref=\"A1\" authorId=\"0\" x=\"1\"><text><t>t</t></text></comment></commentList></comments>")).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let e = read(&format!("<comments {ns}><authors/><commentList><comment ref=\"A1\" authorId=\"3\"><text><t>t</t></text></comment></commentList></comments>")).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InvalidData);
    }
    #[test]
    fn vml_garbage_is_ignored() {
        let mut c = vec![Comment::default()];
        apply_vml(b"<xml><v:shape style=\"width:", &mut c).unwrap();
        apply_vml(b"\xff\xfe not xml", &mut c).unwrap();
        apply_vml(b"", &mut c).unwrap();
        assert_eq!(c[0], Comment::default());
        let vml = "<xml><v:shape style=\"width:3in;height:30pt;visibility:visible\"><x:ClientData><x:Row>0</x:Row><x:Column>0</x:Column></x:ClientData></v:shape></xml>";
        apply_vml(vml.as_bytes(), &mut c).unwrap();
        assert_eq!((c[0].width, c[0].height, c[0].visible), (288.0, 40.0, true));
    }
}
