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
    let mut xml = stream(workbook, "workbook");
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 2 && frame.scope == Scope::Spreadsheet => {
                let e = e.into_owned();
                match e.local_name().as_ref().as_bytes() {
                    b"definedNames" => {
                        features.defined_names = super::defined_name::read(&mut xml, &e, 2)?;
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
    let mut workbook = HEAD.to_vec();
    workbook.extend_from_slice(format!("<workbook xmlns=\"{MAIN}\">").as_bytes());
    if let Some(protection) = &features.protection {
        super::protection::write_workbook(&mut workbook, protection, None).map_err(io_error)?;
    }
    super::defined_name::write(&mut workbook, &features.defined_names, None).map_err(io_error)?;
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
