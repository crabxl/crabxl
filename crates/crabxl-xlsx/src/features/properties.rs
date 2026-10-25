//! Document property parts: `docProps/core.xml`, `docProps/app.xml` and
//! `docProps/custom.xml`.
//!
//! Part layouts follow openpyxl `packaging/{core,extended,custom}.py`.
use crate::encode::{write_attribute, write_text};
use crate::xml::{XmlStream, append_xml_text, attribute};
use crabxl_core::features::{CustomProperty, CustomPropertyValue, DocumentProperties};
use crabxl_core::{Error, ErrorKind, Result};
use quick_xml::events::Event;
use std::io::{self, BufRead, Write};

const MAX_TEXT: usize = 1 << 20;
const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
const CORE_NS: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const DCTERMS_NS: &str = "http://purl.org/dc/terms/";
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
const APP_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
const CUSTOM_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
const VT_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
const CUSTOM_FMTID: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

/// Read the children of the root element, calling `visit(local name, text)`
/// for each direct child. Nested elements contribute no text.
fn read_children<B: BufRead>(
    xml: &mut XmlStream<B>,
    mut visit: impl FnMut(&str, String),
) -> Result<()> {
    let mut current: Option<String> = None;
    let mut text = String::new();
    loop {
        let frame = xml.next()?;
        match &frame.event {
            Event::Start(e) if frame.depth == 2 => {
                current = Some(String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into());
                text.clear();
            }
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) if frame.depth == 2 => {
                append_xml_text(&mut text, &frame.event, MAX_TEXT)?;
            }
            Event::End(_) if frame.depth == 1 => {
                if let Some(name) = current.take() {
                    visit(&name, std::mem::take(&mut text));
                }
            }
            Event::End(_) if frame.depth == 0 => return Ok(()),
            Event::Eof => return Err(invalid("Incomplete properties part")),
            _ => {}
        }
    }
}

fn flag(text: &str) -> Option<bool> {
    match text.trim() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}
fn number(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

/// Read core properties into `props`.
pub(crate) fn read_core<B: BufRead>(
    xml: &mut XmlStream<B>,
    props: &mut DocumentProperties,
) -> Result<()> {
    read_children(xml, |name, text| {
        let slot = match name {
            "creator" => &mut props.creator,
            "title" => &mut props.title,
            "description" => &mut props.description,
            "subject" => &mut props.subject,
            "identifier" => &mut props.identifier,
            "language" => &mut props.language,
            "created" => &mut props.created,
            "modified" => &mut props.modified,
            "lastModifiedBy" => &mut props.last_modified_by,
            "category" => &mut props.category,
            "contentStatus" => &mut props.content_status,
            "version" => &mut props.version,
            "revision" => &mut props.revision,
            "keywords" => &mut props.keywords,
            "lastPrinted" => &mut props.last_printed,
            _ => return,
        };
        *slot = Some(text.into());
    })
}

/// Read extended (app) properties into `props`.
pub(crate) fn read_app<B: BufRead>(
    xml: &mut XmlStream<B>,
    props: &mut DocumentProperties,
) -> Result<()> {
    read_children(xml, |name, text| {
        let boxed = || Some(Box::<str>::from(text.as_str()));
        match name {
            "Application" => props.application = boxed(),
            "AppVersion" => props.app_version = boxed(),
            "Company" => props.company = boxed(),
            "Manager" => props.manager = boxed(),
            "Template" => props.template = boxed(),
            "PresentationFormat" => props.presentation_format = boxed(),
            "HyperlinkBase" => props.hyperlink_base = boxed(),
            "DocSecurity" => props.doc_security = number(&text),
            "ScaleCrop" => props.scale_crop = flag(&text),
            "LinksUpToDate" => props.links_up_to_date = flag(&text),
            "SharedDoc" => props.shared_doc = flag(&text),
            "HyperlinksChanged" => props.hyperlinks_changed = flag(&text),
            "Pages" => props.pages = number(&text),
            "Words" => props.words = number(&text),
            "Characters" => props.characters = number(&text),
            "CharactersWithSpaces" => props.characters_with_spaces = number(&text),
            "Lines" => props.lines = number(&text),
            "Paragraphs" => props.paragraphs = number(&text),
            "Slides" => props.slides = number(&text),
            "Notes" => props.notes = number(&text),
            "TotalTime" => props.total_time = number(&text),
            "HiddenSlides" => props.hidden_slides = number(&text),
            "MMClips" => props.mm_clips = number(&text),
            _ => {}
        }
    })
}

fn custom_value(kind: &str, text: &str) -> Option<CustomPropertyValue> {
    let trimmed = text.trim();
    Some(match kind {
        "lpwstr" | "lpstr" | "bstr" => CustomPropertyValue::String(text.into()),
        "i1" | "i2" | "i4" | "i8" | "int" | "ui1" | "ui2" | "ui4" | "ui8" | "uint" => {
            CustomPropertyValue::Int(trimmed.parse().ok()?)
        }
        "r4" | "r8" | "decimal" => CustomPropertyValue::Float(trimmed.parse().ok()?),
        "bool" => CustomPropertyValue::Bool(flag(trimmed)?),
        "filetime" | "date" => CustomPropertyValue::DateTime(trimmed.into()),
        _ => return None,
    })
}

/// Read custom properties into `props`.
pub(crate) fn read_custom<B: BufRead>(
    xml: &mut XmlStream<B>,
    props: &mut DocumentProperties,
) -> Result<()> {
    let mut name: Option<String> = None;
    let mut link: Option<String> = None;
    let mut value: Option<CustomPropertyValue> = None;
    let mut kind = String::new();
    let mut text = String::new();
    loop {
        let frame = xml.next()?;
        match &frame.event {
            Event::Start(e) if frame.depth == 2 => {
                name = attribute(e, b"name")?;
                link = attribute(e, b"linkTarget")?;
                value = None;
            }
            Event::Start(e) if frame.depth == 3 => {
                kind = String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into();
                text.clear();
            }
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) if frame.depth == 3 => {
                append_xml_text(&mut text, &frame.event, MAX_TEXT)?;
            }
            Event::End(_) if frame.depth == 2 => value = custom_value(&kind, &text),
            Event::End(_) if frame.depth == 1 => {
                let name = name.take();
                let link = link.take();
                let value = value.take();
                if let Some(name) = name {
                    let value = match link {
                        Some(target) => Some(CustomPropertyValue::Link(target.into())),
                        None => value,
                    };
                    if let Some(value) = value {
                        props.custom.push(CustomProperty {
                            name: name.into(),
                            value,
                        });
                    }
                }
            }
            Event::End(_) if frame.depth == 0 => return Ok(()),
            Event::Eof => return Err(invalid("Incomplete custom properties part")),
            _ => {}
        }
    }
}

fn element(out: &mut impl Write, tag: &str, text: &str, extra: &str) -> io::Result<()> {
    write!(out, "<{tag}{extra}>")?;
    write_text(out, text)?;
    write!(out, "</{tag}>")
}

pub(crate) fn write_core(out: &mut impl Write, props: &DocumentProperties) -> io::Result<()> {
    out.write_all(DECL.as_bytes())?;
    write!(out, "<cp:coreProperties xmlns:cp=\"{CORE_NS}\">")?;
    let dc = format!(" xmlns:dc=\"{DC_NS}\"");
    let dated = format!(
        " xmlns:dcterms=\"{DCTERMS_NS}\" xmlns:xsi=\"{XSI_NS}\" xsi:type=\"dcterms:W3CDTF\""
    );
    for (tag, value, extra) in [
        ("dc:creator", &props.creator, &dc),
        ("dc:title", &props.title, &dc),
        ("dc:description", &props.description, &dc),
        ("dc:subject", &props.subject, &dc),
        ("dc:identifier", &props.identifier, &dc),
        ("dc:language", &props.language, &dc),
        ("dcterms:created", &props.created, &dated),
        ("dcterms:modified", &props.modified, &dated),
    ] {
        if let Some(value) = value {
            element(out, tag, value, extra)?;
        }
    }
    for (tag, value) in [
        ("cp:lastModifiedBy", &props.last_modified_by),
        ("cp:category", &props.category),
        ("cp:contentStatus", &props.content_status),
        ("cp:version", &props.version),
        ("cp:revision", &props.revision),
        ("cp:keywords", &props.keywords),
        ("cp:lastPrinted", &props.last_printed),
    ] {
        if let Some(value) = value {
            element(out, tag, value, "")?;
        }
    }
    out.write_all(b"</cp:coreProperties>")
}

/// `sheet_names` are the worksheet titles in workbook order. openpyxl does not
/// write `TitlesOfParts`, so they are unused.
pub(crate) fn write_app(
    out: &mut impl Write,
    props: &DocumentProperties,
    _sheet_names: &[&str],
) -> io::Result<()> {
    out.write_all(DECL.as_bytes())?;
    write!(out, "<Properties xmlns=\"{APP_NS}\">")?;
    let text = |v: &Option<Box<str>>| v.as_deref().map(str::to_owned);
    let boolean = |v: Option<bool>| v.map(|b| if b { "true" } else { "false" }.to_owned());
    let count = |v: Option<u32>| v.map(|n| n.to_string());
    let items: [(&str, Option<String>); 23] = [
        ("Application", text(&props.application)),
        ("AppVersion", text(&props.app_version)),
        ("DocSecurity", count(props.doc_security)),
        ("ScaleCrop", boolean(props.scale_crop)),
        ("LinksUpToDate", boolean(props.links_up_to_date)),
        ("SharedDoc", boolean(props.shared_doc)),
        ("HyperlinksChanged", boolean(props.hyperlinks_changed)),
        ("Template", text(&props.template)),
        ("Manager", text(&props.manager)),
        ("Company", text(&props.company)),
        ("Pages", count(props.pages)),
        ("Words", count(props.words)),
        ("Characters", count(props.characters)),
        ("PresentationFormat", text(&props.presentation_format)),
        ("Lines", count(props.lines)),
        ("Paragraphs", count(props.paragraphs)),
        ("Slides", count(props.slides)),
        ("Notes", count(props.notes)),
        ("TotalTime", count(props.total_time)),
        ("HiddenSlides", count(props.hidden_slides)),
        ("MMClips", count(props.mm_clips)),
        ("CharactersWithSpaces", count(props.characters_with_spaces)),
        ("HyperlinkBase", text(&props.hyperlink_base)),
    ];
    for (tag, value) in items {
        if let Some(value) = value {
            element(out, tag, &value, "")?;
        }
    }
    out.write_all(b"</Properties>")
}

/// True when a custom properties part must be written.
pub(crate) fn has_custom(props: &DocumentProperties) -> bool {
    !props.custom.is_empty()
}

pub(crate) fn write_custom(out: &mut impl Write, props: &DocumentProperties) -> io::Result<()> {
    out.write_all(DECL.as_bytes())?;
    write!(out, "<Properties xmlns=\"{CUSTOM_NS}\">")?;
    for (index, property) in props.custom.iter().enumerate() {
        out.write_all(b"<property")?;
        write_attribute(out, "name", &property.name)?;
        if let CustomPropertyValue::Link(target) = &property.value {
            write_attribute(out, "linkTarget", target)?;
        }
        write_attribute(out, "fmtid", CUSTOM_FMTID)?;
        write_attribute(out, "pid", &(index + 2).to_string())?;
        out.write_all(b">")?;
        let vt = format!(" xmlns:vt=\"{VT_NS}\"");
        match &property.value {
            CustomPropertyValue::String(s) => element(out, "vt:lpwstr", s, &vt)?,
            CustomPropertyValue::Int(n) if i32::try_from(*n).is_ok() => {
                element(out, "vt:i4", &n.to_string(), &vt)?
            }
            CustomPropertyValue::Int(n) => element(out, "vt:i8", &n.to_string(), &vt)?,
            CustomPropertyValue::Float(f) => element(out, "vt:r8", &f.to_string(), &vt)?,
            CustomPropertyValue::Bool(b) => {
                element(out, "vt:bool", if *b { "1" } else { "0" }, &vt)?
            }
            CustomPropertyValue::DateTime(d) => element(out, "vt:filetime", d, &vt)?,
            CustomPropertyValue::Link(_) => element(out, "vt:lpwstr", "", &vt)?,
        }
        out.write_all(b"</property>")?;
    }
    out.write_all(b"</Properties>")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crabxl_core::ResourceLimits;
    use std::io::Cursor;

    fn stream(xml: &str) -> XmlStream<Cursor<Vec<u8>>> {
        XmlStream::new(
            Cursor::new(xml.as_bytes().to_vec()),
            "t".into(),
            1 << 20,
            ResourceLimits::default(),
        )
    }

    const CORE: &str = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"><dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/">openpyxl</dc:creator><dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">T&amp;</dc:title><dc:description xmlns:dc="http://purl.org/dc/elements/1.1/">D</dc:description><dc:subject xmlns:dc="http://purl.org/dc/elements/1.1/">S</dc:subject><dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/">id</dc:identifier><dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language><dcterms:created xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="dcterms:W3CDTF">2021-01-02T03:04:05Z</dcterms:created><dcterms:modified xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="dcterms:W3CDTF">2026-10-25T04:06:49Z</dcterms:modified><cp:lastModifiedBy>me</cp:lastModifiedBy><cp:category>cat</cp:category><cp:contentStatus>draft</cp:contentStatus><cp:version>1.0</cp:version><cp:revision>3</cp:revision><cp:keywords>k</cp:keywords><cp:lastPrinted>2020-01-02T03:04:05Z</cp:lastPrinted></cp:coreProperties>"#;
    const APP: &str = r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Excel Compatible / Openpyxl 3.1.5</Application><AppVersion>3.1</AppVersion></Properties>"#;
    const CUSTOM: &str = r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"><property name="s" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2"><vt:lpwstr xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">hello &lt;x&gt;</vt:lpwstr></property><property name="i" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="3"><vt:i4 xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">5</vt:i4></property><property name="f" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="4"><vt:r8 xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">1.5</vt:r8></property><property name="b" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="5"><vt:bool xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">1</vt:bool></property><property name="d" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="6"><vt:filetime xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">2020-05-06T07:08:09Z</vt:filetime></property><property name="l" linkTarget="Sheet1!A1" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="7"><vt:lpwstr xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"></vt:lpwstr></property></Properties>"#;

    fn body(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes)
            .unwrap()
            .trim_start_matches(DECL)
            .to_owned()
    }

    #[test]
    fn core_round_trip() {
        let mut props = DocumentProperties::default();
        read_core(&mut stream(CORE), &mut props).unwrap();
        assert_eq!(props.title.as_deref(), Some("T&"));
        assert_eq!(props.last_modified_by.as_deref(), Some("me"));
        assert_eq!(props.last_printed.as_deref(), Some("2020-01-02T03:04:05Z"));
        assert_eq!(props.created.as_deref(), Some("2021-01-02T03:04:05Z"));
        let mut out = Vec::new();
        write_core(&mut out, &props).unwrap();
        assert_eq!(body(out), CORE);
    }

    #[test]
    fn app_round_trip() {
        let mut props = DocumentProperties::default();
        read_app(&mut stream(APP), &mut props).unwrap();
        assert_eq!(props.app_version.as_deref(), Some("3.1"));
        props.company = Some("ACME".into());
        props.scale_crop = Some(false);
        props.pages = Some(3);
        let mut out = Vec::new();
        write_app(&mut out, &props, &["A"]).unwrap();
        let mut again = DocumentProperties::default();
        read_app(&mut stream(&body(out)), &mut again).unwrap();
        assert_eq!(again, props);
        let mut plain = Vec::new();
        write_app(
            &mut plain,
            &DocumentProperties::with_openpyxl_defaults("x"),
            &[],
        )
        .unwrap();
        assert_eq!(body(plain), APP);
    }

    #[test]
    fn custom_round_trip() {
        let mut props = DocumentProperties::default();
        assert!(!has_custom(&props));
        read_custom(&mut stream(CUSTOM), &mut props).unwrap();
        assert_eq!(props.custom.len(), 6);
        assert_eq!(
            props.custom[0].value,
            CustomPropertyValue::String("hello <x>".into())
        );
        assert_eq!(props.custom[1].value, CustomPropertyValue::Int(5));
        assert_eq!(props.custom[2].value, CustomPropertyValue::Float(1.5));
        assert_eq!(props.custom[3].value, CustomPropertyValue::Bool(true));
        assert_eq!(
            props.custom[4].value,
            CustomPropertyValue::DateTime("2020-05-06T07:08:09Z".into())
        );
        assert_eq!(
            props.custom[5].value,
            CustomPropertyValue::Link("Sheet1!A1".into())
        );
        assert!(has_custom(&props));
        let mut out = Vec::new();
        write_custom(&mut out, &props).unwrap();
        assert_eq!(body(out), CUSTOM);
    }

    #[test]
    fn unknown_elements_are_skipped() {
        let xml = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"><cp:unknown>x</cp:unknown><cp:keywords>k</cp:keywords></cp:coreProperties>"#;
        let mut props = DocumentProperties::default();
        read_core(&mut stream(xml), &mut props).unwrap();
        assert_eq!(props.keywords.as_deref(), Some("k"));
        props.keywords = None;
        assert!(props.is_empty());
    }
}
