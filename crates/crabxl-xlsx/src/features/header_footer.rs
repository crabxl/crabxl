//! `<headerFooter>` codec.
use super::{invalid, unsupported};
use crate::encode::{write_attribute, write_text};
use crate::xml::{Scope, XmlStream, append_xml_text};
use crabxl_core::features::HeaderFooter;
use crabxl_core::{Error, ErrorKind, Result};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

const MAX_TEXT: usize = 1 << 16;
const CHILDREN: [&str; 6] = [
    "oddHeader",
    "oddFooter",
    "evenHeader",
    "evenFooter",
    "firstHeader",
    "firstFooter",
];

fn field(value: &mut HeaderFooter, index: usize) -> &mut Option<Box<str>> {
    match index {
        0 => &mut value.odd_header,
        1 => &mut value.odd_footer,
        2 => &mut value.even_header,
        3 => &mut value.even_footer,
        4 => &mut value.first_header,
        _ => &mut value.first_footer,
    }
}

fn boolean(value: &str) -> Result<bool> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(invalid("Invalid headerFooter boolean")),
    }
}

/// Read `<headerFooter>`; `start` is the consumed start event at `depth`.
pub(crate) fn read<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<HeaderFooter> {
    let mut value = HeaderFooter::default();
    for attribute in start.attributes() {
        let attribute =
            attribute.map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid XML attribute", e))?;
        let key = attribute.key.as_ref();
        if key == "xmlns" || key.starts_with("xmlns:") {
            continue;
        }
        let text = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid XML attribute value", e))?;
        let flag = match attribute.key.local_name().as_ref().as_bytes() {
            b"differentOddEven" => &mut value.different_odd_even,
            b"differentFirst" => &mut value.different_first,
            b"scaleWithDoc" => &mut value.scale_with_doc,
            b"alignWithMargins" => &mut value.align_with_margins,
            _ => return Err(unsupported("Unknown headerFooter attribute")),
        };
        *flag = Some(boolean(&text)?);
    }
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == depth + 1 => {
                let local = e.local_name();
                let index = CHILDREN
                    .iter()
                    .position(|name| name.as_bytes() == local.as_ref().as_bytes());
                let Some(index) = index.filter(|_| frame.scope == Scope::Spreadsheet) else {
                    return Err(unsupported("Unknown headerFooter child"));
                };
                let mut text = String::new();
                loop {
                    let inner = xml.next()?;
                    match &inner.event {
                        Event::Start(_) => return Err(unsupported("Unknown headerFooter content")),
                        Event::End(_) if inner.depth == depth => break,
                        Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                            append_xml_text(&mut text, &inner.event, MAX_TEXT)?
                        }
                        Event::Eof => return Err(invalid("Incomplete headerFooter")),
                        _ => {}
                    }
                }
                *field(&mut value, index) = Some(text.into());
            }
            Event::Start(_) => return Err(unsupported("Unknown headerFooter child")),
            Event::End(_) if frame.depth + 1 == depth => return Ok(value),
            Event::Eof => return Err(invalid("Incomplete headerFooter")),
            _ => {}
        }
    }
}

/// Write `<headerFooter>`; nothing when the value is empty.
pub(crate) fn write(
    out: &mut impl Write,
    header_footer: &HeaderFooter,
    uri: Option<&str>,
) -> io::Result<()> {
    if header_footer.is_empty() {
        return Ok(());
    }
    out.write_all(b"<headerFooter")?;
    crate::printing::namespace(out, uri)?;
    for (name, flag) in [
        ("differentOddEven", header_footer.different_odd_even),
        ("differentFirst", header_footer.different_first),
        ("scaleWithDoc", header_footer.scale_with_doc),
        ("alignWithMargins", header_footer.align_with_margins),
    ] {
        if let Some(flag) = flag {
            write_attribute(out, name, if flag { "1" } else { "0" })?;
        }
    }
    out.write_all(b">")?;
    for (name, text) in CHILDREN.iter().zip([
        &header_footer.odd_header,
        &header_footer.odd_footer,
        &header_footer.even_header,
        &header_footer.even_footer,
        &header_footer.first_header,
        &header_footer.first_footer,
    ]) {
        if let Some(text) = text {
            write!(out, "<{name}>")?;
            write_text(out, text)?;
            write!(out, "</{name}>")?;
        }
    }
    out.write_all(b"</headerFooter>")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crabxl_core::ResourceLimits;
    use crabxl_core::features::HeaderFooterSections;
    use std::io::Cursor;

    fn parse(xml: &str) -> Result<HeaderFooter> {
        let xml = if xml.contains("xmlns") {
            xml.to_owned()
        } else {
            xml.replacen(
                "<headerFooter",
                "<headerFooter xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"",
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

    const OPENPYXL: &str = r#"<headerFooter differentOddEven="1" differentFirst="1" scaleWithDoc="0" alignWithMargins="0"><oddHeader>&amp;LL&amp;C&amp;"Arial,Bold"&amp;12 &amp;KFF0000C</oddHeader><oddFooter>&amp;RPage &amp;P of &amp;N</oddFooter><evenHeader>&amp;Leven</evenHeader><evenFooter></evenFooter><firstHeader></firstHeader><firstFooter>&amp;Cfirst</firstFooter></headerFooter>"#;

    #[test]
    fn openpyxl_round_trip() {
        let hf = parse(OPENPYXL).unwrap();
        assert_eq!(hf.different_odd_even, Some(true));
        assert_eq!(hf.scale_with_doc, Some(false));
        assert_eq!(
            hf.odd_header.as_deref(),
            Some("&LL&C&\"Arial,Bold\"&12 &KFF0000C")
        );
        assert_eq!(hf.even_footer.as_deref(), Some(""));
        let center = HeaderFooterSections::parse(hf.odd_header.as_deref().unwrap_or(""))
            .center
            .unwrap_or_default();
        assert_eq!(center.size, Some(12));
        let mut out = Vec::new();
        write(&mut out, &hf, None).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), OPENPYXL);
    }

    #[test]
    fn strict_namespace_and_escaping() {
        let hf = HeaderFooter {
            odd_footer: Some("&C<a>&\"x\"".into()),
            ..Default::default()
        };
        let uri = "http://purl.oclc.org/ooxml/spreadsheetml/main";
        let mut out = Vec::new();
        write(&mut out, &hf, Some(uri)).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(uri));
        assert_eq!(parse(&text).unwrap(), hf);
        let mut empty = Vec::new();
        write(&mut empty, &HeaderFooter::default(), None).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn unknown_items_are_rejected() {
        for xml in [
            "<headerFooter><foo/></headerFooter>",
            "<headerFooter bar=\"1\"></headerFooter>",
            "<headerFooter><oddHeader><b/></oddHeader></headerFooter>",
        ] {
            assert_eq!(parse(xml).unwrap_err().kind(), ErrorKind::Unsupported);
        }
    }
}
