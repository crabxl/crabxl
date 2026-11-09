//! DrawingML picture codecs adapted from umya-spreadsheet 3.1.0.
//! Copyright (c) 2020 MathNya. MIT; see third_party/licenses/umya-spreadsheet-MIT.txt.
use crate::xml::{Scope, XmlStream, append_xml_text, attribute, required_attribute};
use crabxl_core::{DrawingAnchor, DrawingExtent, DrawingMarker, Image, Result};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

pub(crate) const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
pub(crate) const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub(crate) const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(crate) const REL_DRAWING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
pub(crate) const REL_IMAGE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub(crate) const CT_DRAWING: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
pub(crate) const CT_RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";
const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";

pub(crate) fn read_chartsheet<B: BufRead>(
    xml: &mut XmlStream<B>,
) -> Result<crabxl_core::drawing::XmlElement> {
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(start)
                if frame.depth == 1
                    && frame.scope == Scope::Spreadsheet
                    && start.local_name().as_ref() == "chartsheet" =>
            {
                let start = start.into_owned();
                let root = crate::drawing_xml::read(xml, &start, 1)?;
                loop {
                    let frame = xml.next()?;
                    match frame.event {
                        Event::Eof => return Ok(root),
                        Event::Text(text)
                            if text.as_ref().bytes().all(|byte| byte.is_ascii_whitespace()) => {}
                        Event::Comment(_) | Event::PI(_) => {}
                        _ => {
                            return Err(crate::features::invalid(
                                "Trailing chartsheet document content",
                            ));
                        }
                    }
                }
            }
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) => {}
            Event::Text(text) if text.as_ref().bytes().all(|byte| byte.is_ascii_whitespace()) => {}
            _ => return Err(crate::features::invalid("Invalid chartsheet root")),
        }
    }
}

pub(crate) fn chartsheet_namespace(root: &crabxl_core::XmlElement) -> Result<&'static str> {
    let attribute = root.name.split_once(':').map_or_else(
        || "xmlns".to_owned(),
        |(prefix, _)| format!("xmlns:{prefix}"),
    );
    match root.attribute(&attribute) {
        Some(crate::xml::MAIN_URI) => Ok(crate::xml::MAIN_URI),
        Some(crate::xml::STRICT_MAIN_URI) => Ok(crate::xml::STRICT_MAIN_URI),
        _ => Err(invalid("Invalid chartsheet namespace")),
    }
}

pub(crate) fn validate_chartsheet_copy(
    root: &crabxl_core::XmlElement,
    graph: Option<&crabxl_core::PartGraph>,
) -> Result<()> {
    if root.local_name() != "chartsheet" {
        return Err(invalid("Invalid chartsheet properties root"));
    }
    let mut properties = root.clone();
    properties.children.retain(|node| !matches!(node, crabxl_core::XmlContent::Element(element) if element.local_name() == "drawing"));
    let ids = graph
        .into_iter()
        .flat_map(|graph| graph.relationships.iter().map(|rel| rel.id.as_ref()))
        .collect();
    chart_relationships(&properties, &ids)
}

pub(crate) fn write_chartsheet(
    output: &mut impl Write,
    source: Option<crabxl_core::drawing::XmlElement>,
    drawing: Option<&str>,
    uri: &str,
) -> io::Result<()> {
    use crabxl_core::drawing::{XmlContent, XmlElement};
    let mut root = source.unwrap_or_else(|| {
        let mut root = XmlElement::new("chartsheet");
        root.set_attribute("xmlns", uri);
        let mut views = XmlElement::new("sheetViews");
        let mut view = XmlElement::new("sheetView");
        view.set_attribute("workbookViewId", "0");
        view.set_attribute("zoomToFit", "1");
        views.children.push(XmlContent::Element(view));
        root.children.push(XmlContent::Element(views));
        root
    });
    root.children.retain(
        |node| !matches!(node, XmlContent::Element(element) if element.local_name() == "drawing"),
    );
    if let Some(id) = drawing {
        let mut element = XmlElement::new("drawing");
        element.set_attribute("xmlns", uri);
        element.set_attribute(
            "xmlns:r",
            if uri == crate::xml::STRICT_MAIN_URI {
                crate::xml::STRICT_OFFICE_REL_URI
            } else {
                R
            },
        );
        element.set_attribute("r:id", id);
        let position = root.children.iter().position(|node| matches!(node, XmlContent::Element(element) if matches!(element.local_name(), "legacyDrawing" | "legacyDrawingHF" | "picture" | "webPublishItems" | "extLst"))).unwrap_or(root.children.len());
        root.children.insert(position, XmlContent::Element(element));
    }
    crate::drawing_xml::write(output, &root, 0)
}

#[derive(Clone, Copy)]
enum Object<'a> {
    Image(&'a Image),
    Chart(&'a crabxl_core::drawing::Chart),
    Shape(&'a crabxl_core::Shape),
}
fn objects<'a>(
    images: &'a [Image],
    charts: &'a [crabxl_core::drawing::Chart],
    shapes: &'a [crabxl_core::Shape],
) -> Vec<Object<'a>> {
    let mut objects: Vec<_> = images
        .iter()
        .map(Object::Image)
        .chain(charts.iter().map(Object::Chart))
        .chain(shapes.iter().map(Object::Shape))
        .collect();
    objects.sort_by_key(|object| match object {
        Object::Image(image) => image.order,
        Object::Chart(chart) => chart.order,
        Object::Shape(shape) => shape.order,
    });
    objects
}

fn write_chart_frame(
    out: &mut impl Write,
    chart: &crabxl_core::drawing::Chart,
    id: usize,
) -> io::Result<()> {
    crate::encode::validate_xml_text(&chart.name).map_err(io::Error::other)?;
    if let Some(frame) = &chart.frame {
        let mut frame = frame.clone();
        let nv = frame
            .child_mut("nvGraphicFramePr")
            .and_then(|v| v.child_mut("cNvPr"))
            .ok_or_else(|| io::Error::other("Chart frame has no nonvisual properties"))?;
        nv.set_attribute("id", id.to_string());
        nv.set_attribute("name", chart.name.as_ref());
        let reference = frame
            .child_mut("graphic")
            .and_then(|v| v.child_mut("graphicData"))
            .and_then(|v| v.child_mut("chart"))
            .ok_or_else(|| io::Error::other("Chart frame has no chart reference"))?;
        let key = reference
            .attributes
            .iter()
            .find(|(key, _)| key.rsplit(':').next() == Some("id"))
            .map(|(key, _)| key.clone())
            .ok_or_else(|| io::Error::other("Chart frame has no chart relationship"))?;
        reference.set_attribute(key, format!("rId{id}"));
        return crate::drawing_xml::write(out, &frame, 0);
    }
    write!(
        out,
        "<xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" r:id=\"rId{id}\"/></a:graphicData></a:graphic></xdr:graphicFrame>",
        quick_xml::escape::escape(chart.name.as_ref())
    )
}

fn object_relationship_ids(
    element: &crabxl_core::XmlElement,
    ids: &mut std::collections::HashSet<String>,
) {
    for (name, value) in &element.attributes {
        if name.contains(':')
            && matches!(name.rsplit(':').next(), Some("id" | "embed" | "link"))
            && !name.starts_with("xmlns:")
        {
            ids.insert(value.to_string());
        }
    }
    for child in &element.children {
        if let crabxl_core::XmlContent::Element(child) = child {
            object_relationship_ids(child, ids);
        }
    }
}

pub(crate) fn chart_relationships(
    element: &crabxl_core::drawing::XmlElement,
    ids: &std::collections::HashSet<&str>,
) -> Result<()> {
    for (name, value) in &element.attributes {
        if name.contains(':')
            && !name.starts_with("xmlns:")
            && matches!(name.rsplit(':').next(), Some("id" | "embed" | "link"))
            && !ids.contains(value.as_ref())
        {
            return Err(invalid(&format!(
                "XML element {} references missing relationship {}",
                element.name, value
            )));
        }
    }
    for child in &element.children {
        if let crabxl_core::drawing::XmlContent::Element(child) = child {
            chart_relationships(child, ids)?;
        }
    }
    Ok(())
}

fn write_chart(out: &mut impl Write, chart: &crabxl_core::drawing::Chart) -> io::Result<()> {
    if chart.space.local_name() != "chartSpace"
        || chart
            .space
            .child("chart")
            .and_then(|v| v.child("plotArea"))
            .is_none()
    {
        return Err(io::Error::other("Invalid chart space"));
    }

    let ids = chart
        .relationships
        .iter()
        .map(|rel| rel.id.as_ref())
        .collect();
    chart_relationships(&chart.space, &ids).map_err(io::Error::other)?;
    out.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>")?;
    crate::drawing_xml::write(out, &chart.space, 0)
}

fn invalid(message: &str) -> crabxl_core::Error {
    crate::features::invalid(message)
}
fn unsupported(message: &str) -> crabxl_core::Error {
    crate::features::unsupported(message)
}
fn number<T: std::str::FromStr>(value: &str) -> Result<T> {
    value
        .parse()
        .map_err(|_| invalid("Invalid drawing coordinate"))
}
fn extent(start: &BytesStart<'_>) -> Result<DrawingExtent> {
    let value = DrawingExtent {
        cx: number(&required_attribute(start, b"cx")?)?,
        cy: number(&required_attribute(start, b"cy")?)?,
    };
    value.validate()?;
    Ok(value)
}

fn read_marker<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<DrawingMarker> {
    let mut values: [Option<String>; 4] = Default::default();
    let mut active = None;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == depth + 1 && frame.scope == Scope::SpreadsheetDrawing =>
            {
                let index = match e.local_name().as_ref().as_bytes() {
                    b"col" => 0,
                    b"colOff" => 1,
                    b"row" => 2,
                    b"rowOff" => 3,
                    _ => return Err(unsupported("Unknown drawing marker child")),
                };
                if values[index].is_some() {
                    return Err(invalid("Duplicate drawing marker child"));
                }
                values[index] = Some(String::new());
                active = Some(index);
            }
            Event::Start(_) => return Err(unsupported("Nested drawing marker content")),
            Event::End(_) if frame.depth + 1 == depth => break,
            Event::End(_) => active = None,
            Event::Eof => return Err(invalid("Truncated drawing marker")),
            ref event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)) => {
                if let Some(index) = active
                    && let Some(value) = &mut values[index]
                {
                    append_xml_text(value, event, 64)?;
                }
            }
            _ => {}
        }
    }
    let get = |index: usize| {
        values[index]
            .as_deref()
            .ok_or_else(|| invalid("Missing drawing marker coordinate"))
    };
    let marker = DrawingMarker {
        col: number(get(0)?)?,
        col_off: number(get(1)?)?,
        row: number(get(2)?)?,
        row_off: number(get(3)?)?,
    };
    marker.validate()?;
    Ok(marker)
}

pub(crate) struct PictureRef {
    pub anchor: DrawingAnchor,
    pub is_chart: bool,
    pub is_shape: bool,
    pub order: usize,
    pub relationship: String,
    pub name: Box<str>,
    pub description: Option<Box<str>>,
    pub properties: crabxl_core::drawing::XmlElement,
    pub client_data: Option<crabxl_core::drawing::XmlElement>,
}

/// Parse picture-only drawings. Other objects reject rather than disappear on replacement.
pub(crate) fn read<B: BufRead>(xml: &mut XmlStream<B>) -> Result<Vec<PictureRef>> {
    let mut pictures = Vec::new();
    let mut root = None;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::SpreadsheetDrawing
                    || e.local_name().as_ref().as_bytes() != b"wsDr"
                {
                    return Err(invalid("Invalid spreadsheet drawing root"));
                }
                root = Some(crate::drawing_xml::element(&e)?);
            }
            Event::Start(e) if frame.depth == 2 && frame.scope == Scope::SpreadsheetDrawing => {
                let e = e.into_owned();
                let mut picture = read_anchor(xml, &e)?;
                if let Some(parent) = &root {
                    crate::drawing_xml::inherit(&mut picture.properties, parent);
                    if let Some(client_data) = &mut picture.client_data {
                        crate::drawing_xml::inherit(client_data, parent);
                    }
                }
                picture.order = pictures.len();
                pictures.push(picture);
            }
            Event::Start(_) => return Err(unsupported("Unknown drawing content")),
            Event::Eof if root.is_some() => return Ok(pictures),
            Event::Eof => return Err(invalid("Missing drawing root")),
            _ => {}
        }
    }
}

fn read_anchor<B: BufRead>(xml: &mut XmlStream<B>, start: &BytesStart<'_>) -> Result<PictureRef> {
    let kind = start.local_name().as_ref().to_owned();
    if !matches!(
        kind.as_str(),
        "oneCellAnchor" | "twoCellAnchor" | "absoluteAnchor"
    ) {
        return Err(unsupported("Unknown drawing anchor"));
    }
    let edit_as = attribute(start, b"editAs")?
        .map(|v| v.parse())
        .transpose()?;
    let mut client_data = None;
    let mut is_chart = false;
    let mut is_shape = false;
    let (mut from, mut to, mut size, mut position, mut picture) = (None, None, None, None, None);
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 3 && frame.scope == Scope::SpreadsheetDrawing => {
                let e = e.into_owned();
                match e.local_name().as_ref().as_bytes() {
                    b"from" if from.is_none() => from = Some(read_marker(xml, 3)?),
                    b"to" if to.is_none() => to = Some(read_marker(xml, 3)?),
                    b"ext" if size.is_none() => {
                        size = Some(extent(&e)?);
                        crate::style_codec::skip(xml, 3)?;
                    }
                    b"pos" if position.is_none() => {
                        position = Some((
                            number(&required_attribute(&e, b"x")?)?,
                            number(&required_attribute(&e, b"y")?)?,
                        ));
                        crate::style_codec::skip(xml, 3)?;
                    }
                    b"pic" if picture.is_none() => {
                        picture = Some(crate::drawing_xml::read(xml, &e, 3)?)
                    }
                    b"graphicFrame" if picture.is_none() => {
                        is_chart = true;
                        picture = Some(crate::drawing_xml::read(xml, &e, 3)?);
                    }
                    b"clientData" if client_data.is_none() => {
                        client_data = Some(crate::drawing_xml::read(xml, &e, 3)?)
                    }
                    b"sp" | b"cxnSp" | b"grpSp" | b"contentPart" if picture.is_none() => {
                        is_shape = true;
                        picture = Some(crate::drawing_xml::read(xml, &e, 3)?);
                    }
                    _ => return Err(unsupported("Unsupported or duplicate drawing object")),
                }
            }
            Event::Start(_) => return Err(unsupported("Unsupported drawing anchor content")),
            Event::End(_) if frame.depth == 1 => break,
            Event::Eof => return Err(invalid("Truncated drawing anchor")),
            _ => {}
        }
    }
    let missing = || invalid("Incomplete drawing anchor");
    let anchor = match kind.as_str() {
        "oneCellAnchor" => DrawingAnchor::OneCell {
            from: from.ok_or_else(missing)?,
            extent: size.ok_or_else(missing)?,
        },
        "twoCellAnchor" => DrawingAnchor::TwoCell {
            from: from.ok_or_else(missing)?,
            to: to.ok_or_else(missing)?,
            edit_as,
        },
        _ => {
            let (x, y) = position.ok_or_else(missing)?;
            DrawingAnchor::Absolute {
                x,
                y,
                extent: size.ok_or_else(missing)?,
            }
        }
    };
    let mut properties = picture.ok_or_else(|| invalid("Drawing anchor has no object"))?;
    let anchor_scope = crate::drawing_xml::element(start)?;
    crate::drawing_xml::inherit(&mut properties, &anchor_scope);
    if let Some(client_data) = &mut client_data {
        crate::drawing_xml::inherit(client_data, &anchor_scope);
    }
    if is_chart
        && properties
            .child("graphic")
            .and_then(|v| v.child("graphicData"))
            .and_then(|v| v.child("chart"))
            .is_none()
    {
        is_chart = false;
        is_shape = true;
    }
    let (relationship, name, description) = if is_shape {
        (String::new(), Box::<str>::from("Shape"), None)
    } else if is_chart {
        let nv = properties
            .child("nvGraphicFramePr")
            .and_then(|v| v.child("cNvPr"))
            .ok_or_else(|| invalid("Chart frame has no nonvisual properties"))?;
        let reference = properties
            .child("graphic")
            .and_then(|v| v.child("graphicData"))
            .and_then(|v| v.child("chart"))
            .ok_or_else(|| unsupported("Graphic frame does not contain a chart"))?;
        let relationship = reference
            .attributes
            .iter()
            .find(|(key, _)| key.rsplit(':').next() == Some("id"))
            .map(|(_, v)| v.to_string())
            .ok_or_else(|| invalid("Chart frame has no relationship"))?;
        (
            relationship,
            nv.attribute("name").unwrap_or("Chart").into(),
            nv.attribute("descr").map(Into::into),
        )
    } else {
        picture_properties(&properties)?
    };
    Ok(PictureRef {
        anchor,
        relationship,
        name,
        description,
        properties,
        client_data,
        is_chart,
        is_shape,
        order: 0,
    })
}

fn picture_properties(
    element: &crabxl_core::drawing::XmlElement,
) -> Result<(String, Box<str>, Option<Box<str>>)> {
    let nv = element
        .child("nvPicPr")
        .and_then(|v| v.child("cNvPr"))
        .ok_or_else(|| invalid("Picture has no nonvisual properties"))?;
    let name = nv
        .attribute("name")
        .ok_or_else(|| invalid("Picture has no name"))?
        .into();
    let description = nv.attribute("descr").map(Into::into);
    let blip = element
        .child("blipFill")
        .and_then(|v| v.child("blip"))
        .ok_or_else(|| invalid("Picture has no blip"))?;
    let mut embeds = blip
        .attributes
        .iter()
        .filter(|(key, _)| key.contains(':') && key.rsplit(':').next() == Some("embed"));
    let relationship = embeds
        .next()
        .map(|(_, value)| value.to_string())
        .ok_or_else(|| unsupported("Picture has no embedded image relationship"))?;
    if embeds.next().is_some() {
        return Err(invalid("Picture has multiple image relationships"));
    }
    Ok((relationship, name, description))
}

fn write_picture(out: &mut impl Write, image: &Image, id: usize) -> io::Result<()> {
    if let Some(properties) = &image.properties {
        picture_properties(properties).map_err(io::Error::other)?;
        let mut properties = properties.clone();
        let nv = properties
            .child_mut("nvPicPr")
            .and_then(|v| v.child_mut("cNvPr"))
            .ok_or_else(|| io::Error::other("Picture has no nonvisual properties"))?;
        nv.set_attribute("id", id.to_string());
        nv.set_attribute("name", image.name.as_ref());
        nv.attributes.retain(|(key, _)| key.as_ref() != "descr");
        if let Some(description) = &image.description {
            nv.set_attribute("descr", description.as_ref());
        }
        let blip = properties
            .child_mut("blipFill")
            .and_then(|v| v.child_mut("blip"))
            .ok_or_else(|| io::Error::other("Picture has no blip"))?;
        let (key, _) = blip
            .attributes
            .iter_mut()
            .find(|(key, _)| key.rsplit(':').next() == Some("embed"))
            .ok_or_else(|| io::Error::other("Picture has no image relationship"))?;
        let key = key.clone();
        blip.set_attribute(key, format!("rId{id}"));
        return crate::drawing_xml::write(out, &properties, 0);
    }
    write!(
        out,
        "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"{}\"",
        quick_xml::escape::escape(image.name.as_ref())
    )?;
    if let Some(description) = &image.description {
        write!(
            out,
            " descr=\"{}\"",
            quick_xml::escape::escape(description.as_ref())
        )?;
    }
    write!(
        out,
        "/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"rId{id}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"
    )
}

fn marker(out: &mut impl Write, name: &str, value: &DrawingMarker) -> io::Result<()> {
    write!(
        out,
        "<xdr:{name}><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:{name}>",
        value.col, value.col_off, value.row, value.row_off
    )
}
fn write_extent(out: &mut impl Write, value: &DrawingExtent) -> io::Result<()> {
    write!(out, "<xdr:ext cx=\"{}\" cy=\"{}\"/>", value.cx, value.cy)
}

pub(crate) fn write(
    out: &mut impl Write,
    images: &[Image],
    charts: &[crabxl_core::drawing::Chart],
    shapes: &[crabxl_core::Shape],
) -> io::Result<()> {
    write!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><xdr:wsDr xmlns:xdr=\"{XDR}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\">"
    )?;
    for (index, object) in objects(images, charts, shapes).into_iter().enumerate() {
        let (anchor, client_data) = match object {
            Object::Image(image) => (&image.anchor, &image.client_data),
            Object::Chart(chart) => (&chart.anchor, &chart.client_data),
            Object::Shape(shape) => (&shape.anchor, &shape.client_data),
        };
        anchor.validate().map_err(io::Error::other)?;
        let tag = match anchor {
            DrawingAnchor::OneCell { from, extent } => {
                out.write_all(b"<xdr:oneCellAnchor>")?;
                marker(out, "from", from)?;
                write_extent(out, extent)?;
                "oneCellAnchor"
            }
            DrawingAnchor::TwoCell { from, to, edit_as } => {
                out.write_all(b"<xdr:twoCellAnchor")?;
                if let Some(edit_as) = edit_as {
                    write!(out, " editAs=\"{}\"", edit_as.as_str())?;
                }
                out.write_all(b">")?;
                marker(out, "from", from)?;
                marker(out, "to", to)?;
                "twoCellAnchor"
            }
            DrawingAnchor::Absolute { x, y, extent } => {
                write!(out, "<xdr:absoluteAnchor><xdr:pos x=\"{x}\" y=\"{y}\"/>")?;
                write_extent(out, extent)?;
                "absoluteAnchor"
            }
        };
        let id = index + 1;
        match object {
            Object::Image(image) => {
                if image.data.is_empty() {
                    return Err(io::Error::other("Empty image payload"));
                }
                crate::encode::validate_xml_text(&image.name).map_err(io::Error::other)?;
                if let Some(description) = &image.description {
                    crate::encode::validate_xml_text(description).map_err(io::Error::other)?;
                }
                write_picture(out, image, id)?;
            }
            Object::Chart(chart) => write_chart_frame(out, chart, id)?,
            Object::Shape(shape) => crate::drawing_xml::write(out, &shape.properties, 0)?,
        }
        if let Some(client_data) = client_data {
            crate::drawing_xml::write(out, client_data, 0)?;
        } else {
            out.write_all(b"<xdr:clientData/>")?;
        }
        write!(out, "</xdr:{tag}>")?;
    }
    out.write_all(b"</xdr:wsDr>")
}

fn remap_shape(
    element: &mut crabxl_core::XmlElement,
    ids: &std::collections::HashMap<String, String>,
    relationships: bool,
) -> io::Result<()> {
    for (name, value) in &mut element.attributes {
        let reference = if relationships {
            name.contains(':')
                && !name.starts_with("xmlns:")
                && matches!(name.rsplit(':').next(), Some("id" | "embed" | "link"))
        } else {
            name.as_ref() == "id"
                && matches!(element.name.rsplit(':').next(), Some("stCxn" | "endCxn"))
        };
        if reference {
            if let Some(replacement) = ids.get(value.as_ref()) {
                *value = replacement.as_str().into();
            } else if !relationships {
                return Err(io::Error::other(
                    "Drawing object references a missing identity",
                ));
            }
        }
    }
    for child in &mut element.children {
        if let crabxl_core::XmlContent::Element(child) = child {
            remap_shape(child, ids, relationships)?;
        }
    }
    Ok(())
}
fn renumber_shape(
    element: &mut crabxl_core::XmlElement,
    next: &mut usize,
    ids: &mut std::collections::HashMap<String, String>,
) {
    if element.local_name() == "cNvPr" {
        let id = next.to_string();
        *next += 1;
        if let Some(old) = element.attribute("id") {
            ids.entry(old.to_owned()).or_insert_with(|| id.clone());
        }
        element.set_attribute("id", id);
    }
    for child in &mut element.children {
        if let crabxl_core::XmlContent::Element(child) = child {
            renumber_shape(child, next, ids);
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn prepare_shapes(
    images: &[Image],
    charts: &[crabxl_core::Chart],
    shapes: &mut [crabxl_core::Shape],
    root: &str,
    numbers: &mut crate::features::package::PartNumbers,
    output: &mut crate::features::package::SheetOutput,
    prefix: &str,
    primary: Option<&str>,
) -> io::Result<Vec<u8>> {
    use std::collections::HashMap;
    use std::io::Read;
    let mut global = HashMap::new();
    let objects = objects(images, charts, shapes);
    for (index, object) in objects.iter().enumerate() {
        let properties = match object {
            Object::Image(image) => image.properties.as_ref().and_then(|v| v.child("nvPicPr")),
            Object::Chart(chart) => chart
                .frame
                .as_ref()
                .and_then(|v| v.child("nvGraphicFramePr")),
            Object::Shape(_) => None,
        };
        if let Some(old) = properties
            .and_then(|v| v.child("cNvPr"))
            .and_then(|v| v.attribute("id"))
        {
            global
                .entry(old.to_owned())
                .or_insert_with(|| (index + 1).to_string());
        }
    }
    let mut next = objects.len() + 1;
    drop(objects);
    let mut local = Vec::new();
    for shape in shapes.iter_mut() {
        let mut ids = HashMap::new();
        renumber_shape(&mut shape.properties, &mut next, &mut ids);
        for (old, new) in &ids {
            global.entry(old.clone()).or_insert_with(|| new.clone());
        }
        local.push(ids);
    }
    let mut extra = Vec::new();
    for (index, (shape, mut ids)) in shapes.iter_mut().zip(local).enumerate() {
        for (old, new) in &global {
            ids.entry(old.clone()).or_insert_with(|| new.clone());
        }
        remap_shape(&mut shape.properties, &ids, false)?;
        let mut known: std::collections::HashSet<_> = shape
            .relationships
            .iter()
            .map(|rel| rel.id.as_ref())
            .collect();
        known.extend(primary);
        chart_relationships(&shape.properties, &known).map_err(io::Error::other)?;
        let mapping: HashMap<_, _> = shape
            .relationships
            .iter()
            .map(|rel| {
                (
                    rel.id.to_string(),
                    format!("rIdShape{prefix}{index}_{}", rel.id),
                )
            })
            .collect();
        remap_shape(&mut shape.properties, &mapping, true)?;
        let mut parts = crate::features::package::SheetOutput::default();
        build_part_dependencies(
            &shape.relationships,
            &shape.parts,
            shape.source_part.as_deref(),
            root,
            numbers,
            &mut parts,
        )?;
        let rels_part = crate::package::relationship_part(root);
        for part in parts.parts {
            if part.name != rels_part {
                output.parts.push(part);
                continue;
            }
            let mut bytes = Vec::new();
            part.bytes.reader().read_to_end(&mut bytes)?;
            let mut xml = XmlStream::new(
                io::Cursor::new(bytes.as_slice()),
                rels_part.clone(),
                bytes.len() as u64,
                Default::default(),
            );
            loop {
                let frame = xml.next().map_err(io::Error::other)?;
                match frame.event {
                    Event::Start(e) if frame.depth == 2 && frame.scope == Scope::Relationships => {
                        let mut element =
                            crate::drawing_xml::element(&e).map_err(io::Error::other)?;
                        let id = element
                            .attribute("Id")
                            .and_then(|id| mapping.get(id))
                            .ok_or_else(|| {
                                io::Error::other("Missing shape relationship identity")
                            })?;
                        element.set_attribute("Id", id.as_str());
                        crate::drawing_xml::write(&mut extra, &element, 0)?;
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
    }
    Ok(extra)
}

#[allow(clippy::too_many_arguments)]
fn prepare_object_parts(
    properties: &mut crabxl_core::XmlElement,
    graph: &crabxl_core::PartGraph,
    anchor: &DrawingAnchor,
    root: &str,
    numbers: &mut crate::features::package::PartNumbers,
    output: &mut crate::features::package::SheetOutput,
    prefix: &str,
    primary: &str,
) -> io::Result<Vec<u8>> {
    let mut shape = crabxl_core::Shape {
        anchor: anchor.clone(),
        properties: properties.clone(),
        client_data: None,
        order: 0,
        relationships: graph.relationships.clone(),
        parts: graph.parts.clone(),
        source_part: graph.source_part.clone(),
    };
    let extra = prepare_shapes(
        &[],
        &[],
        std::slice::from_mut(&mut shape),
        root,
        numbers,
        output,
        prefix,
        Some(primary),
    )?;
    *properties = shape.properties;
    Ok(extra)
}

pub(crate) fn build_output(
    images: &[Image],
    charts: &[crabxl_core::drawing::Chart],
    shapes: &[crabxl_core::Shape],
    sheet_part: &str,
    numbers: &mut crate::features::package::PartNumbers,
    relationship_id: &mut impl FnMut(&str) -> String,
    output: &mut crate::features::package::SheetOutput,
) -> io::Result<()> {
    use crate::features::package::{OutputPart, OutputRelationship, relative_target};
    if images.is_empty() && charts.is_empty() && shapes.is_empty() {
        return Ok(());
    }
    let part = numbers.next_name(|n| &mut n.drawing_file, ("xl/drawings/drawing", ".xml"));
    let id = relationship_id("drawing");
    output.refs.drawing = Some(id.clone());
    output.relationships.push(OutputRelationship {
        id,
        kind: REL_DRAWING.into(),
        target: relative_target(sheet_part, &part),
    });
    let mut shapes = shapes.to_vec();
    let mut extra_relationships = prepare_shapes(
        images,
        charts,
        &mut shapes,
        &part,
        numbers,
        output,
        "S",
        None,
    )?;
    let mut images = images.to_vec();
    let mut charts = charts.to_vec();
    for (index, image) in images.iter_mut().enumerate() {
        if let Some(properties) = &mut image.properties {
            let primary = picture_properties(properties).map_err(io::Error::other)?.0;
            extra_relationships.extend(prepare_object_parts(
                properties,
                &image.drawing_parts,
                &image.anchor,
                &part,
                numbers,
                output,
                &format!("I{index}_"),
                &primary,
            )?);
        }
    }
    for (index, chart) in charts.iter_mut().enumerate() {
        if let Some(frame) = &mut chart.frame {
            let primary = frame
                .child("graphic")
                .and_then(|v| v.child("graphicData"))
                .and_then(|v| v.child("chart"))
                .and_then(|v| {
                    v.attributes
                        .iter()
                        .find(|(key, _)| key.rsplit(':').next() == Some("id"))
                })
                .map(|(_, value)| value.to_string())
                .ok_or_else(|| io::Error::other("Missing chart frame relationship"))?;
            extra_relationships.extend(prepare_object_parts(
                frame,
                &chart.drawing_parts,
                &chart.anchor,
                &part,
                numbers,
                output,
                &format!("C{index}_"),
                &primary,
            )?);
        }
    }
    let images = images.as_slice();
    let charts = charts.as_slice();
    let shapes = shapes.as_slice();
    let mut bytes = Vec::new();
    write(&mut bytes, images, charts, shapes)?;
    output.parts.push(OutputPart {
        name: part.clone(),
        content_type: CT_DRAWING.into(),
        bytes: bytes.into(),
    });
    let mut rels = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">".to_vec();
    for (index, object) in objects(images, charts, shapes).into_iter().enumerate() {
        let (name, kind, content_type, bytes) = match object {
            Object::Image(image) => {
                let extension = format!(".{}", image.format.extension());
                let name = numbers.next_name(|n| &mut n.image_file, ("xl/media/image", &extension));
                (
                    name,
                    REL_IMAGE,
                    image.format.content_type(),
                    image.data.clone(),
                )
            }
            Object::Shape(_) => continue,
            Object::Chart(chart) => {
                let name = numbers.next_name(|n| &mut n.chart_file, ("xl/charts/chart", ".xml"));
                let mut bytes = Vec::new();
                write_chart(&mut bytes, chart)?;
                build_part_dependencies(
                    &chart.relationships,
                    &chart.parts,
                    chart.source_part.as_deref(),
                    &name,
                    numbers,
                    output,
                )?;
                (name, REL_CHART, CT_CHART, bytes.into())
            }
        };
        OutputRelationship {
            id: format!("rId{}", index + 1),
            kind: kind.into(),
            target: relative_target(&part, &name),
        }
        .write(&mut rels)?;
        output.parts.push(OutputPart {
            name,
            content_type: content_type.into(),
            bytes,
        });
    }
    rels.extend_from_slice(&extra_relationships);
    rels.extend_from_slice(b"</Relationships>");
    output.parts.push(OutputPart {
        name: crate::package::relationship_part(&part),
        content_type: CT_RELS.into(),
        bytes: rels.into(),
    });
    Ok(())
}

impl<Rd: std::io::Read + std::io::Seek> crate::WorkbookReader<Rd> {
    pub(crate) fn sheet_drawing_part(&mut self, sheet: &str) -> Result<Option<String>> {
        let info = self
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| {
                crabxl_core::Error::new(
                    crabxl_core::ErrorKind::SheetNotFound,
                    "Drawing sheet not found",
                )
            })?;
        let part = info.part().to_owned();
        let mut xml = self.part_stream(&part)?;
        let mut drawing = None;
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Spreadsheet
                        && e.local_name().as_ref() == "drawing" =>
                {
                    if drawing.is_some() {
                        return Err(invalid("Duplicate sheet drawing"));
                    }
                    drawing = Some(
                        frame
                            .office_relationship
                            .ok_or_else(|| invalid("Drawing has no relationship id"))?
                            .to_owned(),
                    );
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Spreadsheet
                        && e.local_name().as_ref() == "sheetData" =>
                {
                    crate::style_codec::skip(&mut xml, 2)?;
                }
                Event::Eof => break,
                _ => {}
            }
        }
        drop(xml);
        let Some(id) = drawing else {
            return Ok(None);
        };
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &crate::package::relationship_part(&part),
            self.limits,
            &mut remaining,
        )?;
        let rel = relationships
            .get(&id)
            .ok_or_else(|| invalid("Sheet drawing relationship is missing"))?;
        if rel.external || rel.kind.rsplit('/').next() != Some("drawing") {
            return Err(invalid("Invalid sheet drawing relationship"));
        }
        Ok(Some(crate::package::resolve_part(&part, &rel.target)?))
    }
    pub(crate) fn drawing_relationships(
        &mut self,
        part: &str,
        remaining: &mut u64,
    ) -> Result<Vec<crabxl_core::drawing::PartRelationship>> {
        let rels_part = crate::package::relationship_part(part);
        if self.archive.index_for_name(&rels_part).is_none() {
            return Ok(Vec::new());
        }
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &rels_part,
            self.limits,
            remaining,
        )?;
        let mut output = Vec::with_capacity(relationships.len());
        for (id, rel) in relationships {
            if !rel.external
                && matches!(
                    rel.kind.rsplit('/').next(),
                    Some("worksheet" | "chartsheet" | "officeDocument" | "sharedStrings")
                )
            {
                return Err(unsupported(
                    "Drawing dependency points into a cell workbook",
                ));
            }
            output.push(crabxl_core::drawing::PartRelationship {
                id: id.into(),
                kind: rel.kind.into(),
                target: if rel.external {
                    rel.target.into()
                } else {
                    crate::package::resolve_part(part, &rel.target)?.into()
                },
                external: rel.external,
            });
        }
        output.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(output)
    }

    pub(crate) fn read_part_dependencies(
        &mut self,
        root: &str,
    ) -> Result<(
        Vec<crabxl_core::drawing::PartRelationship>,
        Vec<crabxl_core::drawing::RelatedPart>,
    )> {
        self.read_part_dependencies_without(root, &[])
    }
    pub(crate) fn read_part_dependencies_without(
        &mut self,
        root: &str,
        excluded: &[&str],
    ) -> Result<(
        Vec<crabxl_core::drawing::PartRelationship>,
        Vec<crabxl_core::drawing::RelatedPart>,
    )> {
        self.read_part_dependencies_selected(root, excluded, None)
    }
    fn read_part_dependencies_selected(
        &mut self,
        root: &str,
        excluded: &[&str],
        selected: Option<&std::collections::HashSet<String>>,
    ) -> Result<(
        Vec<crabxl_core::drawing::PartRelationship>,
        Vec<crabxl_core::drawing::RelatedPart>,
    )> {
        use std::collections::{HashMap, HashSet, VecDeque};
        let mut types = HashMap::new();
        let mut defaults = HashMap::new();
        {
            let mut xml = self.part_stream("[Content_Types].xml")?;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e) if frame.depth == 2 && frame.scope == Scope::ContentTypes => {
                        match e.local_name().as_ref().as_bytes() {
                            b"Override" => {
                                types.insert(
                                    crate::package::resolve_part(
                                        "",
                                        &required_attribute(&e, b"PartName")?,
                                    )?,
                                    required_attribute(&e, b"ContentType")?,
                                );
                            }
                            b"Default" => {
                                defaults.insert(
                                    required_attribute(&e, b"Extension")?.to_ascii_lowercase(),
                                    required_attribute(&e, b"ContentType")?,
                                );
                            }
                            _ => return Err(unsupported("Unknown content type declaration")),
                        }
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
        let mut remaining = self.limits.max_metadata_bytes;
        let mut relationships = self.drawing_relationships(root, &mut remaining)?;
        if let Some(selected) = selected {
            relationships.retain(|rel| selected.contains(rel.id.as_ref()));
        }
        let mut seen = HashSet::new();
        seen.insert(root.to_owned());
        seen.extend(excluded.iter().map(|name| (*name).to_owned()));
        let mut queue: VecDeque<_> = relationships
            .iter()
            .filter(|rel| !rel.external)
            .map(|rel| rel.target.to_string())
            .collect();
        let mut parts = Vec::new();
        let mut bytes = 0u64;
        let mut memory = 0usize;
        while let Some(name) = queue.pop_front() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let content_type = types
                .get(&name)
                .or_else(|| {
                    defaults.get(&name.rsplit('.').next().unwrap_or("").to_ascii_lowercase())
                })
                .ok_or_else(|| invalid("Drawing dependency has no content type"))?
                .clone();
            let size = self
                .archive
                .by_name(&name)
                .map_err(|error| {
                    crabxl_core::Error::caused_by(
                        crabxl_core::ErrorKind::Archive,
                        "Missing drawing dependency",
                        error,
                    )
                    .with_part(&name)
                })?
                .size();
            bytes = bytes
                .checked_add(size)
                .ok_or_else(|| invalid("Drawing payload size overflow"))?;
            if bytes > self.limits.max_total_uncompressed_bytes {
                return Err(crabxl_core::Error::new(
                    crabxl_core::ErrorKind::LimitExceeded,
                    "Encoded dependencies exceed total byte limit",
                ));
            }
            let data = self.part_payload(&name)?;
            memory = memory.saturating_add(data.memory_bytes());
            if memory > self.limits.max_materialized_bytes {
                return Err(crabxl_core::Error::new(
                    crabxl_core::ErrorKind::MemoryBudgetExceeded,
                    "Encoded dependencies exceed materialization allowance",
                ));
            }
            let related = self.drawing_relationships(&name, &mut remaining)?;
            queue.extend(
                related
                    .iter()
                    .filter(|rel| !rel.external)
                    .map(|rel| rel.target.to_string()),
            );
            parts.push(crabxl_core::drawing::RelatedPart {
                name: name.into(),
                content_type: content_type.into(),
                data,
                relationships: related,
            });
        }
        Ok((relationships, parts))
    }

    pub(crate) fn read_object_parts(
        &mut self,
        root: &str,
        properties: &crabxl_core::XmlElement,
        primary: &str,
        all: bool,
    ) -> Result<crabxl_core::PartGraph> {
        let mut ids = std::collections::HashSet::new();
        object_relationship_ids(properties, &mut ids);
        if all {
            let mut remaining = self.limits.max_metadata_bytes;
            ids.extend(
                self.drawing_relationships(root, &mut remaining)?
                    .iter()
                    .map(|rel| rel.id.to_string()),
            );
        }
        ids.remove(primary);
        let (relationships, parts) = self.read_part_dependencies_selected(root, &[], Some(&ids))?;
        let mut known: std::collections::HashSet<_> =
            relationships.iter().map(|rel| rel.id.as_ref()).collect();
        known.insert(primary);
        chart_relationships(properties, &known)?;
        Ok(crabxl_core::PartGraph {
            source_part: Some(root.into()),
            relationships,
            parts,
        })
    }
    /// Read shapes and grouped objects with their full property and dependency graphs.
    pub fn sheet_shapes(&mut self, sheet: &str) -> Result<Vec<crabxl_core::Shape>> {
        let Some(part) = self.sheet_drawing_part(sheet)? else {
            return Ok(Vec::new());
        };
        let references = read(&mut self.part_stream(&part)?)?;
        let mut shapes = Vec::new();
        for reference in references
            .into_iter()
            .filter(|reference| reference.is_shape)
        {
            let mut ids = std::collections::HashSet::new();
            object_relationship_ids(&reference.properties, &mut ids);
            let (relationships, parts) =
                self.read_part_dependencies_selected(&part, &[], Some(&ids))?;
            let known = relationships.iter().map(|rel| rel.id.as_ref()).collect();
            chart_relationships(&reference.properties, &known)?;
            shapes.push(crabxl_core::Shape {
                anchor: reference.anchor,
                properties: reference.properties,
                client_data: reference.client_data,
                order: reference.order,
                source_part: Some(part.clone().into()),
                relationships,
                parts,
            });
        }
        Ok(shapes)
    }

    /// Read chart property trees and anchors without materializing cells.
    pub fn sheet_charts(&mut self, sheet: &str) -> Result<Vec<crabxl_core::drawing::Chart>> {
        let Some(part) = self.sheet_drawing_part(sheet)? else {
            return Ok(Vec::new());
        };
        let references = read(&mut self.part_stream(&part)?)?;
        let rels_part = crate::package::relationship_part(&part);
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &rels_part,
            self.limits,
            &mut remaining,
        )?;
        let mut charts = Vec::new();
        for reference in references
            .into_iter()
            .filter(|reference| reference.is_chart && !reference.is_shape)
        {
            let rel = relationships
                .get(&reference.relationship)
                .ok_or_else(|| invalid("Missing chart relationship"))?;
            if rel.external || rel.kind.rsplit('/').next() != Some("chart") {
                return Err(invalid("Invalid chart relationship"));
            }
            let target = crate::package::resolve_part(&part, &rel.target)?;
            let mut xml = self.part_stream(&target)?;
            let mut space = None;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e)
                        if frame.depth == 1
                            && frame.scope == Scope::Chart
                            && e.local_name().as_ref() == "chartSpace" =>
                    {
                        let e = e.into_owned();
                        space = Some(crate::drawing_xml::read(&mut xml, &e, 1)?);
                    }
                    Event::Start(_) => return Err(invalid("Invalid chart document")),
                    Event::Eof => break,
                    _ => {}
                }
            }
            let space = space.ok_or_else(|| invalid("Missing chart space"))?;
            drop(xml);
            let (chart_relationships, related_parts) = self.read_part_dependencies(&target)?;
            let drawing_parts = self.read_object_parts(
                &part,
                &reference.properties,
                &reference.relationship,
                false,
            )?;
            charts.push(crabxl_core::drawing::Chart {
                drawing_parts,
                anchor: reference.anchor,
                space,
                name: reference.name,
                frame: Some(reference.properties),
                client_data: reference.client_data,
                order: reference.order,
                relationships: chart_relationships,
                parts: related_parts,
                source_part: Some(target.into()),
            });
        }
        Ok(charts)
    }
    fn image_format(&mut self, part: &str) -> Result<crabxl_core::ImageFormat> {
        if let Ok(format) =
            crabxl_core::ImageFormat::from_extension(part.rsplit('.').next().unwrap_or(""))
        {
            return Ok(format);
        }
        let extension = part.rsplit('.').next().unwrap_or("");
        let mut xml = self.part_stream("[Content_Types].xml")?;
        let mut kind = None;
        let mut default = None;
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e) if frame.depth == 2 && frame.scope == Scope::ContentTypes => {
                    match e.local_name().as_ref() {
                        "Override"
                            if crate::package::resolve_part(
                                "",
                                &required_attribute(&e, b"PartName")?,
                            )? == part =>
                        {
                            kind = Some(required_attribute(&e, b"ContentType")?)
                        }
                        "Default"
                            if required_attribute(&e, b"Extension")?
                                .eq_ignore_ascii_case(extension) =>
                        {
                            default = Some(required_attribute(&e, b"ContentType")?)
                        }
                        _ => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        crabxl_core::ImageFormat::from_content_type(
            kind.or(default)
                .as_deref()
                .ok_or_else(|| invalid("Image has no content type"))?,
        )
    }
    /// Read worksheet pictures and anchors without materializing cells.
    /// Drawings containing other object types fail explicitly.
    pub fn sheet_images(&mut self, sheet: &str) -> Result<Vec<Image>> {
        let Some(part) = self.sheet_drawing_part(sheet)? else {
            return Ok(Vec::new());
        };
        let pictures = read(&mut self.part_stream(&part)?)?;
        let rels_part = crate::package::relationship_part(&part);
        let mut remaining = self.limits.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.archive,
            &rels_part,
            self.limits,
            &mut remaining,
        )?;
        let mut payloads = std::collections::HashMap::<String, crabxl_core::BinaryPayload>::new();
        let mut images = Vec::with_capacity(pictures.len());
        for picture in pictures
            .into_iter()
            .filter(|picture| !picture.is_chart && !picture.is_shape)
        {
            let relationship = relationships
                .get(&picture.relationship)
                .ok_or_else(|| invalid("Missing image relationship"))?;
            if relationship.external || relationship.kind.rsplit('/').next() != Some("image") {
                return Err(unsupported("Picture does not reference an internal image"));
            }
            let target = crate::package::resolve_part(&part, &relationship.target)?;
            let format = self.image_format(&target)?;
            let data = match payloads.get(&target) {
                Some(data) => data.clone(),
                None => {
                    let data = self.part_payload(&target)?;
                    payloads.insert(target, data.clone());
                    data
                }
            };
            let drawing_parts =
                self.read_object_parts(&part, &picture.properties, &picture.relationship, false)?;
            let mut image = Image::new(data, format, picture.anchor)?;
            image.drawing_parts = drawing_parts;
            image.name = picture.name;
            image.description = picture.description;
            image.properties = Some(picture.properties);
            image.client_data = picture.client_data;
            image.order = picture.order;
            images.push(image);
        }
        Ok(images)
    }
}

pub(crate) fn take_relationships(
    parts: &mut Vec<crate::features::package::OutputPart>,
    root: &str,
) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let name = crate::package::relationship_part(root);
    let Some(index) = parts.iter().position(|part| part.name == name) else {
        return Ok(Vec::new());
    };
    let part = parts.remove(index);
    let mut bytes = Vec::new();
    part.bytes.reader().read_to_end(&mut bytes)?;
    let mut xml = XmlStream::new(
        io::Cursor::new(bytes.as_slice()),
        name,
        bytes.len() as u64,
        Default::default(),
    );
    let mut output = Vec::new();
    loop {
        let frame = xml.next().map_err(io::Error::other)?;
        match frame.event {
            Event::Start(e) if frame.depth == 2 && frame.scope == Scope::Relationships => {
                let element = crate::drawing_xml::element(&e).map_err(io::Error::other)?;
                crate::drawing_xml::write(&mut output, &element, 0)?;
            }
            Event::Eof => return Ok(output),
            _ => {}
        }
    }
}

pub(crate) fn build_part_dependencies(
    root_relationships: &[crabxl_core::drawing::PartRelationship],
    parts: &[crabxl_core::drawing::RelatedPart],
    source_part: Option<&str>,
    root: &str,
    numbers: &mut crate::features::package::PartNumbers,
    output: &mut crate::features::package::SheetOutput,
) -> io::Result<()> {
    build_part_dependencies_mapped(
        root_relationships,
        parts,
        source_part,
        root,
        numbers,
        output,
        &[],
    )
}

pub(crate) fn build_part_dependencies_mapped(
    root_relationships: &[crabxl_core::drawing::PartRelationship],
    parts: &[crabxl_core::drawing::RelatedPart],
    source_part: Option<&str>,
    root: &str,
    numbers: &mut crate::features::package::PartNumbers,
    output: &mut crate::features::package::SheetOutput,
    existing: &[(&str, &str)],
) -> io::Result<()> {
    use crate::features::package::{OutputPart, relative_target};
    let mut names: std::collections::HashMap<_, _> = existing
        .iter()
        .map(|(source, target)| (*source, (*target).to_owned()))
        .collect();
    if let Some(source) = source_part {
        names.insert(source, root.to_owned());
    }
    for part in parts {
        if names.contains_key(part.name.as_ref()) {
            return Err(io::Error::other("Duplicate chart dependency identity"));
        }
        let extension = part.name.rsplit('.').next().unwrap_or("bin");
        if !extension.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(io::Error::other("Invalid chart dependency extension"));
        }
        let suffix = format!(".{extension}");
        names.insert(
            part.name.as_ref(),
            numbers.next_name(|n| &mut n.related_file, ("xl/charts/parts/part", &suffix)),
        );
    }
    let mut relationships = |source: &str,
                             values: &[crabxl_core::drawing::PartRelationship]|
     -> io::Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let mut bytes = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">".to_vec();
        let mut ids = std::collections::HashSet::new();
        for rel in values {
            if rel.id.is_empty() || !ids.insert(rel.id.as_ref()) {
                return Err(io::Error::other(
                    "Invalid or duplicate chart relationship id",
                ));
            }
            for value in [rel.id.as_ref(), rel.kind.as_ref(), rel.target.as_ref()] {
                crate::encode::validate_xml_text(value).map_err(io::Error::other)?;
            }
            let target = if rel.external {
                rel.target.to_string()
            } else {
                relative_target(
                    source,
                    names
                        .get(rel.target.as_ref())
                        .ok_or_else(|| io::Error::other("Missing chart dependency target"))?,
                )
            };
            write!(
                bytes,
                "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>",
                quick_xml::escape::escape(rel.id.as_ref()),
                quick_xml::escape::escape(rel.kind.as_ref()),
                quick_xml::escape::escape(&target),
                if rel.external {
                    " TargetMode=\"External\""
                } else {
                    ""
                }
            )?;
        }
        bytes.extend_from_slice(b"</Relationships>");
        output.parts.push(OutputPart {
            name: crate::package::relationship_part(source),
            content_type: CT_RELS.into(),
            bytes: bytes.into(),
        });
        Ok(())
    };
    relationships(root, root_relationships)?;
    for part in parts {
        let name = names
            .get(part.name.as_ref())
            .ok_or_else(|| io::Error::other("Missing chart dependency allocation"))?;
        relationships(name, &part.relationships)?;
    }
    for part in parts {
        crate::encode::validate_xml_text(&part.content_type).map_err(io::Error::other)?;
        output.parts.push(OutputPart {
            name: names
                .get(part.name.as_ref())
                .ok_or_else(|| io::Error::other("Missing chart dependency allocation"))?
                .clone(),
            content_type: part.content_type.clone(),
            bytes: part.data.clone(),
        });
    }
    Ok(())
}
