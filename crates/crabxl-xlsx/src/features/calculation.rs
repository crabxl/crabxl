//! Calculation property serialization under the enclosing workbook namespace.
use crabxl_core::XmlElement;
use std::io::{self, Write};

pub(crate) fn write(
    out: &mut impl Write,
    value: &XmlElement,
    uri: Option<&str>,
    invalidate: bool,
) -> io::Result<()> {
    if value.local_name() != "calcPr" {
        return Err(io::Error::other("Expected calculation properties"));
    }
    let mut value = value.clone();
    value.name = "calcPr".into();
    value.set_attribute(
        "xmlns",
        uri.unwrap_or("http://schemas.openxmlformats.org/spreadsheetml/2006/main"),
    );
    if invalidate {
        value.set_attribute("calcMode", "auto");
        value.set_attribute("fullCalcOnLoad", "1");
        value.set_attribute("forceFullCalc", "1");
    }
    crate::drawing_xml::write(out, &value, 0)
}
