//! Small DrawingML trees preserve formatting that has no convenience accessor.
use crate::xml::{XmlStream, append_xml_text};
use crabxl_core::{
    Result,
    drawing::{XmlContent, XmlElement},
};
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use std::io::{self, BufRead, Write};

pub(crate) fn element(start: &BytesStart<'_>) -> Result<XmlElement> {
    let mut element = XmlElement::new(start.name().as_ref());
    for attribute in start.attributes() {
        let attribute =
            attribute.map_err(|_| crate::features::invalid("Invalid drawing attribute"))?;
        element.attributes.push((
            attribute.key.as_ref().into(),
            attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|_| crate::features::invalid("Invalid drawing attribute value"))?
                .into_owned()
                .into(),
        ));
    }
    Ok(element)
}

pub(crate) fn read<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<XmlElement> {
    let mut remaining = usize::MAX;
    read_limited(xml, start, depth, &mut remaining)
}

pub(crate) fn read_limited<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
    remaining: &mut usize,
) -> Result<XmlElement> {
    fn charge(remaining: &mut usize, bytes: usize) -> Result<()> {
        *remaining = remaining.checked_sub(bytes).ok_or_else(|| {
            crabxl_core::Error::new(
                crabxl_core::ErrorKind::LimitExceeded,
                "XML property tree byte limit exceeded",
            )
        })?;
        Ok(())
    }
    let mut element = element(start)?;
    charge(remaining, size_of::<XmlElement>() + element.heap_bytes())?;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(start) => {
                let start = start.into_owned();
                let child_depth = frame.depth;
                element.children.push(XmlContent::Element(read_limited(
                    xml,
                    &start,
                    child_depth,
                    remaining,
                )?));
                charge(remaining, size_of::<XmlContent>())?;
            }
            Event::End(_) if frame.depth + 1 == depth => return Ok(element),
            Event::Eof => return Err(crate::features::invalid("Truncated drawing properties")),
            ref event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)) => {
                let mut text = String::new();
                append_xml_text(&mut text, event, *remaining)?;
                charge(remaining, size_of::<XmlContent>() + text.len())?;
                element.children.push(XmlContent::Text(text.into()));
            }
            Event::Comment(value) => {
                charge(remaining, size_of::<XmlContent>() + value.as_ref().len())?;
                element
                    .children
                    .push(XmlContent::Comment(value.as_ref().into()));
            }
            Event::PI(value) => {
                charge(remaining, size_of::<XmlContent>() + value.as_ref().len())?;
                element
                    .children
                    .push(XmlContent::ProcessingInstruction(value.as_ref().into()));
            }
            _ => {
                return Err(crate::features::unsupported(
                    "Unsupported drawing property event",
                ));
            }
        }
    }
}

fn name(value: &str) -> io::Result<()> {
    let mut parts = value.split(':');
    let valid = |part: &str| {
        let mut chars = part.chars();
        chars
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '-' || ch == '.' || ch.is_alphanumeric())
    };
    if !parts.next().is_some_and(valid)
        || !parts.all(valid)
        || value.bytes().filter(|b| *b == b':').count() > 1
    {
        return Err(io::Error::other("Invalid drawing XML name"));
    }
    Ok(())
}

pub(crate) fn write(out: &mut impl Write, element: &XmlElement, depth: usize) -> io::Result<()> {
    write_scoped(
        out,
        element,
        depth,
        &mut quick_xml::name::NamespaceResolver::default(),
    )
}

fn write_scoped(
    out: &mut impl Write,
    element: &XmlElement,
    depth: usize,
    namespaces: &mut quick_xml::name::NamespaceResolver,
) -> io::Result<()> {
    if depth > 256 {
        return Err(io::Error::other("Drawing property nesting exceeds 256"));
    }
    name(&element.name)?;
    let mut start = BytesStart::new(element.name.as_ref());
    let mut names = std::collections::HashSet::new();
    for (key, value) in &element.attributes {
        name(key)?;
        if !names.insert(key.as_ref()) {
            return Err(io::Error::other("Duplicate drawing attribute"));
        }
        crate::encode::validate_xml_text(value).map_err(io::Error::other)?;
        start.push_attribute((key.as_ref(), value.as_ref()));
    }
    namespaces.push(&start).map_err(io::Error::other)?;
    if matches!(
        namespaces.resolve_element(start.name()).0,
        quick_xml::name::ResolveResult::Unknown(_)
    ) {
        return Err(io::Error::other("Undeclared metadata element namespace"));
    }
    let mut expanded = std::collections::HashSet::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(io::Error::other)?;
        if attribute.key.as_namespace_binding().is_some() {
            continue;
        }
        let (namespace, local) = namespaces.resolve_attribute(attribute.key);
        let namespace = match namespace {
            quick_xml::name::ResolveResult::Unknown(_) => {
                return Err(io::Error::other("Undeclared metadata attribute namespace"));
            }
            quick_xml::name::ResolveResult::Bound(namespace) => namespace.as_ref().to_owned(),
            quick_xml::name::ResolveResult::Unbound => String::new(),
        };
        if !expanded.insert((namespace, local.as_ref().to_owned())) {
            return Err(io::Error::other("Duplicate expanded metadata attribute"));
        }
    }
    let mut writer = quick_xml::Writer::new(out);
    writer.write_event(Event::Start(start))?;
    for child in &element.children {
        match child {
            XmlContent::Element(child) => {
                write_scoped(&mut **writer.get_mut(), child, depth + 1, namespaces)?
            }
            XmlContent::Text(text) => {
                crate::encode::validate_xml_text(text).map_err(io::Error::other)?;
                writer.write_event(Event::Text(BytesText::new(text)))?;
            }
            XmlContent::Comment(value) => {
                crate::encode::validate_xml_text(value).map_err(io::Error::other)?;
                if value.contains("--") || value.ends_with('-') {
                    return Err(io::Error::other("Invalid XML comment"));
                }
                writer.write_event(Event::Comment(BytesText::from_escaped(value.as_ref())))?;
            }
            XmlContent::ProcessingInstruction(value) => {
                crate::encode::validate_xml_text(value).map_err(io::Error::other)?;
                let target = value
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| io::Error::other("Empty XML processing instruction"))?;
                name(target)?;
                if target.eq_ignore_ascii_case("xml") || value.contains("?>") {
                    return Err(io::Error::other("Invalid XML processing instruction"));
                }
                writer.write_event(Event::PI(quick_xml::events::BytesPI::new(value.as_ref())))?;
            }
        }
    }
    writer.write_event(Event::End(BytesEnd::new(element.name.as_ref())))?;
    namespaces.pop();
    Ok(())
}

/// Carry inherited prefixes into an independently written subtree.
pub(crate) fn inherit(element: &mut XmlElement, parent: &XmlElement) {
    for (key, value) in &parent.attributes {
        if (key.as_ref() == "xmlns" || key.starts_with("xmlns:"))
            && element.attribute(key).is_none()
        {
            element.attributes.push((key.clone(), value.clone()));
        }
    }
}
