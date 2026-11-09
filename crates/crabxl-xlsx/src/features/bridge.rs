//! Convert features from and to plain SpreadsheetML XML.
//!
//! Language bindings that already model these objects as XML (for example the
//! openpyxl classes in the Python binding) use these functions instead of a
//! field-by-field conversion. Comments are not covered: their size lives in VML,
//! so bindings set `SheetFeatures::comments` directly.
use crate::xml::{Scope, XmlStream};
use crabxl_core::{DifferentialStyle, Result, SheetFeatures, WorkbookFeatures};
use quick_xml::events::Event;
use std::io::{self, Cursor};

fn stream<'a>(bytes: &'a [u8], part: &str) -> XmlStream<Cursor<&'a [u8]>> {
    let limits = crabxl_core::ResourceLimits::default();
    XmlStream::new(Cursor::new(bytes), part.into(), bytes.len() as u64, limits)
}
fn io_error(error: io::Error) -> crabxl_core::Error {
    crabxl_core::Error::caused_by(
        crabxl_core::ErrorKind::Io,
        "Cannot encode feature XML",
        error,
    )
}
const HEAD: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

/// Decode a chartsheet property document without loading worksheet cells.
pub fn chartsheet_from_xml(bytes: &[u8]) -> Result<crabxl_core::XmlElement> {
    let root = spreadsheet_element(bytes, "chartsheet")?;
    crate::drawing::chartsheet_namespace(&root)?;
    Ok(root)
}

/// Encode a complete chartsheet property document for language bindings.
pub fn chartsheet_to_xml(root: &crabxl_core::XmlElement) -> Result<Vec<u8>> {
    crate::drawing::chartsheet_namespace(root)?;
    let mut output = Vec::new();
    crate::drawing_xml::write(&mut output, root, 0).map_err(io_error)?;
    Ok(output)
}

/// Decode a table descriptor while retaining its complete XML metadata.
pub fn pivot_table_from_xml(bytes: &[u8]) -> Result<crabxl_core::XmlElement> {
    spreadsheet_element(bytes, "pivotTableDefinition")
}

/// Decode cache metadata independently of encoded record sources.
pub fn pivot_cache_from_xml(bytes: &[u8]) -> Result<crabxl_core::XmlElement> {
    spreadsheet_element(bytes, "pivotCacheDefinition")
}

/// Decode a cache-record descriptor under the metadata XML limits.
pub fn pivot_records_from_xml(bytes: &[u8]) -> Result<crabxl_core::XmlElement> {
    spreadsheet_element(bytes, "pivotCacheRecords")
}

/// Encode an existing pivot descriptor without regenerating its package graph.
pub fn pivot_definition_to_xml(root: &crabxl_core::XmlElement) -> Result<Vec<u8>> {
    if !matches!(
        root.name.rsplit(':').next(),
        Some("pivotTableDefinition" | "pivotCacheDefinition")
    ) {
        return Err(super::invalid("Invalid pivot definition root"));
    }
    let mut output = Vec::new();
    crate::drawing_xml::write(&mut output, root, 0).map_err(io_error)?;
    Ok(output)
}

fn spreadsheet_element(bytes: &[u8], expected: &str) -> Result<crabxl_core::XmlElement> {
    let mut xml = stream(bytes, expected);
    let mut element = None;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(start)
                if frame.depth == 1
                    && frame.scope == Scope::Spreadsheet
                    && start.local_name().as_ref() == expected
                    && element.is_none() =>
            {
                let start = start.into_owned();
                let mut remaining =
                    usize::try_from(crabxl_core::ResourceLimits::default().max_metadata_bytes)
                        .unwrap_or(usize::MAX);
                element = Some(crate::drawing_xml::read_limited(
                    &mut xml,
                    &start,
                    1,
                    &mut remaining,
                )?);
            }
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Text(text) if text.as_ref().trim().is_empty() => {}
            Event::Eof => return element.ok_or_else(|| super::invalid("Missing pivot definition")),
            _ => return Err(super::invalid("Invalid pivot definition document")),
        }
    }
}

/// Build a canonical pivot graph from a binding's XML descriptor objects.
pub fn pivot_from_xml(
    table: &[u8],
    cache: &[u8],
    records: Option<&[u8]>,
) -> Result<crabxl_core::features::PivotTable> {
    use crabxl_core::features::{PivotCache, PivotTable};
    let mut definition = pivot_table_from_xml(table)?;
    definition
        .attributes
        .retain(|(name, _)| !(name.contains(':') && name.rsplit(':').next() == Some("id")));
    definition.set_attribute(
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    );
    definition.set_attribute("r:id", "rIdCache");
    let id = definition
        .attribute("cacheId")
        .unwrap_or("0")
        .parse()
        .map_err(|_| super::invalid("Invalid pivot cache identity"))?;
    let mut cache = PivotCache {
        id,
        definition: pivot_cache_from_xml(cache)?,
        source_part: None,
        relationships: Vec::new(),
        parts: Vec::new(),
    };
    if let Some(records) = records {
        let document = pivot_records_from_xml(records)?;
        let count = document
            .attribute("count")
            .unwrap_or("0")
            .parse()
            .map_err(|_| super::invalid("Invalid pivot record count"))?;
        cache.set_records(records.into(), count);
    }
    Ok(PivotTable {
        definition,
        cache: std::sync::Arc::new(cache),
        relationships: vec![crabxl_core::drawing::PartRelationship {
            id: "rIdCache".into(),
            kind: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition".into(),
            target: "crabxl/cache".into(),
            external: false,
        }],
        parts: Vec::new(),
        source_part: None,
    })
}

/// Decode worksheet views supplied by a language binding.
pub fn sheet_views_from_xml(bytes: &[u8]) -> Result<crabxl_core::SheetViews> {
    let mut document = format!("<worksheet xmlns=\"{MAIN}\">").into_bytes();
    document.extend_from_slice(bytes);
    document.extend_from_slice(b"<sheetData/></worksheet>");
    let maximum = usize::try_from(crabxl_core::ResourceLimits::default().max_metadata_bytes)
        .unwrap_or(usize::MAX);
    crate::worksheet_view::read_header(&mut stream(&document, "views"), maximum)
}

/// Encode canonical worksheet views for a language binding.
pub fn sheet_views_to_xml(views: &crabxl_core::SheetViews) -> Result<Vec<u8>> {
    crate::worksheet_view::validate(views)?;
    let mut output = Vec::new();
    crate::worksheet_view::write_views(&mut output, views, Some(MAIN)).map_err(io_error)?;
    Ok(output)
}

/// Decode a complete worksheet printing fragment supplied by a binding.
pub fn print_settings_from_xml(bytes: &[u8]) -> Result<crabxl_core::PrintSettings> {
    let maximum = usize::try_from(crabxl_core::ResourceLimits::default().max_metadata_bytes)
        .unwrap_or(usize::MAX);
    crate::printing::read(&mut stream(bytes, "printing"), maximum)
}

/// Encode canonical printing metadata without worksheet cell data.
pub fn print_settings_to_xml(settings: &crabxl_core::PrintSettings) -> Result<Vec<u8>> {
    crate::printing::validate(settings)?;
    let mut output = format!("<worksheet xmlns=\"{MAIN}\">").into_bytes();
    crate::printing::write_properties(&mut output, settings, None).map_err(io_error)?;
    if settings.format.is_some() {
        crate::printing::write_format(&mut output, Some(settings), None).map_err(io_error)?;
    }
    output.extend_from_slice(b"<sheetData/>");
    crate::printing::write_page(&mut output, settings, None).map_err(io_error)?;
    crate::printing::write_breaks(&mut output, "rowBreaks", &settings.row_breaks, None)
        .map_err(io_error)?;
    crate::printing::write_breaks(&mut output, "colBreaks", &settings.column_breaks, None)
        .map_err(io_error)?;
    output.extend_from_slice(b"</worksheet>");
    Ok(output)
}

/// Parse a complete, relationship-free chart part supplied by a binding.
pub fn chart_from_xml(
    bytes: &[u8],
    anchor: crabxl_core::DrawingAnchor,
) -> Result<crabxl_core::Chart> {
    anchor.validate()?;
    let mut xml = stream(bytes, "chart");
    let space = loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(start)
                if frame.scope == Scope::Chart && start.local_name().as_ref() == "chartSpace" =>
            {
                let start = start.into_owned();
                let mut remaining =
                    usize::try_from(crabxl_core::ResourceLimits::default().max_metadata_bytes)
                        .unwrap_or(usize::MAX);
                break crate::drawing_xml::read_limited(&mut xml, &start, 1, &mut remaining)?;
            }
            Event::Decl(_) | Event::Comment(_) | Event::Text(_) => {}
            _ => return Err(super::invalid("Expected a chartSpace document")),
        }
    };
    loop {
        match xml.next()?.event {
            Event::Eof => break,
            Event::Comment(_) | Event::Text(_) => {}
            _ => return Err(super::invalid("Unexpected content after chartSpace")),
        }
    }
    if space
        .child("chart")
        .and_then(|chart| chart.child("plotArea"))
        .is_none()
    {
        return Err(super::invalid("Chart requires a plot area"));
    }
    Ok(crabxl_core::Chart {
        anchor,
        space,
        name: "Chart".into(),
        frame: None,
        drawing_parts: Default::default(),
        client_data: None,
        order: 0,
        relationships: Vec::new(),
        parts: Vec::new(),
        source_part: None,
    })
}

/// Worksheet features as XML.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetFeaturesXml {
    /// A `<worksheet>` document that holds only the feature elements
    /// (sheetProtection, autoFilter, sortState, conditionalFormatting,
    /// dataValidations, headerFooter) in schema order.
    pub worksheet: Vec<u8>,
    /// One complete table part per table, in table order. The table `id`
    /// attribute is the position plus one.
    pub tables: Vec<Vec<u8>>,
}

/// Parse worksheet features. `worksheet` is a `<worksheet>` document with any
/// subset of the feature elements; other worksheet elements are ignored.
/// `tables` are complete table parts. Comments stay empty.
pub fn sheet_features_from_xml(worksheet: &[u8], tables: &[&[u8]]) -> Result<SheetFeatures> {
    let (mut features, _) = super::sheet::read_worksheet(&mut stream(worksheet, "worksheet"))?;
    for table in tables {
        features
            .tables
            .push(super::table::read_part(&mut stream(table, "table"))?);
    }
    Ok(features)
}

/// Serialize worksheet features except comments.
pub fn sheet_features_to_xml(features: &SheetFeatures) -> Result<SheetFeaturesXml> {
    if features.chartsheet_parts.is_some()
        || features.chartsheet.is_some()
        || features.printer_settings.is_some()
        || features.images.is_some()
        || features.charts.is_some()
        || features.shapes.is_some()
        || features.pivots.is_some()
    {
        return Err(super::unsupported(
            "Package-backed features require the workbook writer",
        ));
    }
    let mut worksheet = HEAD.to_vec();
    worksheet.extend_from_slice(format!("<worksheet xmlns=\"{MAIN}\">").as_bytes());
    super::sheet::write_ranks(
        &mut worksheet,
        features,
        &super::sheet::PartRefs::default(),
        0,
        usize::MAX,
        None,
    )
    .map_err(io_error)?;
    worksheet.extend_from_slice(b"</worksheet>");
    let mut tables = Vec::with_capacity(features.tables.len());
    for (index, table) in features.tables.iter().enumerate() {
        let mut bytes = Vec::new();
        super::table::write_part(&mut bytes, table, index as u32 + 1).map_err(io_error)?;
        tables.push(bytes);
    }
    Ok(SheetFeaturesXml { worksheet, tables })
}

/// Workbook features as XML.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkbookFeaturesXml {
    /// A `<workbook>` document with `workbookProtection` and `definedNames`.
    pub workbook: Vec<u8>,
    /// `docProps/core.xml`, when properties are set.
    pub core: Option<Vec<u8>>,
    /// `docProps/app.xml`, when properties are set.
    pub app: Option<Vec<u8>>,
    /// `docProps/custom.xml`, when custom properties are set.
    pub custom: Option<Vec<u8>>,
}

/// Parse workbook features from a `<workbook>` document (only
/// `workbookProtection` and `definedNames` are read) and property parts.
pub fn workbook_features_from_xml(
    workbook: &[u8],
    core: Option<&[u8]>,
    app: Option<&[u8]>,
    custom: Option<&[u8]>,
) -> Result<WorkbookFeatures> {
    let mut features = WorkbookFeatures::default();
    let mut parent = None;
    let calculation_limit =
        usize::try_from(crabxl_core::ResourceLimits::default().max_metadata_bytes)
            .unwrap_or(usize::MAX);
    let mut xml = stream(workbook, "workbook");
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if e.len() > calculation_limit {
                    return Err(crabxl_core::Error::new(
                        crabxl_core::ErrorKind::LimitExceeded,
                        "Workbook namespace context exceeds metadata allowance",
                    ));
                }
                let mut context = crate::drawing_xml::element(&e)?;
                context
                    .attributes
                    .retain(|(name, _)| name.as_ref() == "xmlns" || name.starts_with("xmlns:"));
                parent = Some(context);
            }

            Event::Start(e) if frame.depth == 2 && frame.scope == Scope::Spreadsheet => {
                let e = e.into_owned();
                match e.local_name().as_ref().as_bytes() {
                    b"definedNames" => {
                        features.defined_names = super::defined_name::read(&mut xml, &e, 2)?;
                    }
                    b"calcPr" | b"bookViews" | b"workbookPr" => {
                        if e.len() > calculation_limit {
                            return Err(crabxl_core::Error::new(
                                crabxl_core::ErrorKind::LimitExceeded,
                                "Calculation properties exceed metadata allowance",
                            ));
                        }
                        let mut remaining = calculation_limit;
                        let mut calculation =
                            crate::drawing_xml::read_limited(&mut xml, &e, 2, &mut remaining)?;
                        if let Some(parent) = &parent {
                            crate::drawing_xml::inherit(&mut calculation, parent);
                        }
                        if std::mem::size_of::<crabxl_core::XmlElement>() + calculation.heap_bytes()
                            > calculation_limit
                        {
                            return Err(crabxl_core::Error::new(
                                crabxl_core::ErrorKind::LimitExceeded,
                                "Calculation properties exceed metadata allowance",
                            ));
                        }
                        if e.local_name().as_ref() == "workbookPr" {
                            features.workbook_properties = Some(calculation);
                        } else if e.local_name().as_ref() == "bookViews" {
                            features.views = Some(calculation);
                        } else {
                            features.calculation = Some(calculation);
                        }
                    }
                    b"workbookProtection" => {
                        features.protection =
                            Some(super::protection::read_workbook(&mut xml, &e, 2)?);
                    }
                    _ => crate::style_codec::skip(&mut xml, 2)?,
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if let Some(bytes) = core {
        super::properties::read_core(&mut stream(bytes, "core"), &mut features.properties)?;
    }
    if let Some(bytes) = app {
        super::properties::read_app(&mut stream(bytes, "app"), &mut features.properties)?;
    }
    if let Some(bytes) = custom {
        super::properties::read_custom(&mut stream(bytes, "custom"), &mut features.properties)?;
    }
    Ok(features)
}

/// Serialize workbook features.
pub fn workbook_features_to_xml(features: &WorkbookFeatures) -> Result<WorkbookFeaturesXml> {
    if features.metadata.is_some()
        || features.external_links.is_some()
        || features.format.is_some()
        || !matches!(
            features.macros,
            crabxl_core::features::MacroPolicy::Preserve
        )
    {
        return Err(super::unsupported(
            "Workbook package graphs require the workbook writer",
        ));
    }
    let mut workbook = HEAD.to_vec();
    workbook.extend_from_slice(format!("<workbook xmlns=\"{MAIN}\">").as_bytes());
    if let Some(properties) = &features.workbook_properties {
        super::workbook_properties::write(&mut workbook, properties, None, None)
            .map_err(io_error)?;
    }
    if let Some(views) = &features.views {
        super::book_views::write(&mut workbook, views, None, None).map_err(io_error)?;
    }
    if let Some(protection) = &features.protection {
        super::protection::write_workbook(&mut workbook, protection, None).map_err(io_error)?;
    }
    super::defined_name::write(&mut workbook, &features.defined_names, None).map_err(io_error)?;
    if let Some(calculation) = &features.calculation {
        crate::drawing_xml::write(&mut workbook, calculation, 0).map_err(io_error)?;
    }
    workbook.extend_from_slice(b"</workbook>");
    let props = &features.properties;
    let set = *props != crabxl_core::features::DocumentProperties::default();
    let mut output = WorkbookFeaturesXml {
        workbook,
        ..Default::default()
    };
    if set {
        let mut core = Vec::new();
        super::properties::write_core(&mut core, props).map_err(io_error)?;
        let mut app = Vec::new();
        super::properties::write_app(&mut app, props, &[]).map_err(io_error)?;
        output.core = Some(core);
        output.app = Some(app);
        if super::properties::has_custom(props) {
            let mut custom = Vec::new();
            super::properties::write_custom(&mut custom, props).map_err(io_error)?;
            output.custom = Some(custom);
        }
    }
    Ok(output)
}

/// Parse one `<dxf>` element (differential style of a conditional format).
pub fn differential_style_from_xml(dxf: &[u8]) -> Result<DifferentialStyle> {
    let mut wrapped = format!("<styleSheet xmlns=\"{MAIN}\"><dxfs count=\"1\">").into_bytes();
    let body = dxf
        .iter()
        .position(|byte| *byte == b'<')
        .map_or(dxf, |start| &dxf[start..]);
    let body = if body.starts_with(b"<?") {
        body.iter()
            .position(|byte| *byte == b'>')
            .map_or(body, |end| &body[end + 1..])
    } else {
        body
    };
    wrapped.extend_from_slice(body);
    wrapped.extend_from_slice(b"</dxfs></styleSheet>");
    let mut catalog = crabxl_core::StyleCatalog::default();
    let mut xml = stream(&wrapped, "dxf");
    let mut budget = crate::style_reader::Budget {
        used: 0,
        maximum: 1 << 20,
        records: 1,
    };
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"dxfs" =>
            {
                crate::style_extras_codec::read_differentials(&mut xml, &mut catalog, &mut budget)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    catalog
        .differential_styles
        .pop()
        .filter(|_| catalog.differential_styles.is_empty())
        .ok_or_else(|| super::invalid("Expected one dxf element"))
}

/// Serialize one differential style as a `<dxf>` element.
pub fn differential_style_to_xml(style: &DifferentialStyle) -> Result<Vec<u8>> {
    let mut catalog = crabxl_core::StyleCatalog::default();
    catalog.differential_styles.push(style.clone());
    let mut bytes = Vec::new();
    crate::style_extras_codec::write(
        &mut bytes,
        &catalog,
        crate::StyleWritePolicy::RetainExplicit,
    )
    .map_err(io_error)?;
    let start = bytes
        .windows(5)
        .position(|window| window == b"<dxf>")
        .ok_or_else(|| super::invalid("Missing dxf output"))?;
    let end = bytes
        .windows(7)
        .rposition(|window| window == b"</dxfs>")
        .ok_or_else(|| super::invalid("Missing dxf output"))?;
    Ok(bytes[start..end].to_vec())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn dxf_round_trip() {
        let xml = b"<dxf><font><b val=\"1\"/><color rgb=\"FF9C0006\"/></font><fill><patternFill><bgColor rgb=\"FFFFC7CE\"/></patternFill></fill></dxf>";
        let style = differential_style_from_xml(xml).unwrap();
        assert!(style.font.is_some() && style.fill.is_some());
        let back = differential_style_to_xml(&style).unwrap();
        assert_eq!(differential_style_from_xml(&back).unwrap(), style);
    }
}
