//! Workbook properties retain opaque metadata and respect the canonical date epoch.
use crabxl_core::XmlElement;
use std::io::{self, Write};

pub(crate) fn write(
    out: &mut impl Write,
    value: &XmlElement,
    uri: Option<&str>,
    date_1904: Option<bool>,
) -> io::Result<()> {
    if value.local_name() != "workbookPr" {
        return Err(io::Error::other("Expected workbook properties"));
    }
    let mut value = value.clone();
    value.name = "workbookPr".into();
    value.set_attribute(
        "xmlns",
        uri.unwrap_or("http://schemas.openxmlformats.org/spreadsheetml/2006/main"),
    );
    if let Some(epoch) = date_1904
        && (epoch || value.attribute("date1904").is_some())
    {
        value.set_attribute("date1904", if epoch { "1" } else { "0" });
    }
    crate::drawing_xml::write(out, &value, 0)
}
