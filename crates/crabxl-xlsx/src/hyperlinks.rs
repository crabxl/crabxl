// SPDX-License-Identifier: MIT
// External hyperlink relationship ordering adapted from umya-spreadsheet,
// Copyright (c) 2020 MathNya. Bounded codecs and full optional fields are CrabXL.
//! Point hyperlink metadata and owned-package relationship encoding.
mod scan;
pub(crate) use scan::Capture;

use crate::{
    encode::{RowBuffer, validate_xml_text, write_attribute},
    xml::OFFICE_REL_URI,
};
use crabxl_core::{Error, ErrorKind, Hyperlinks, Result};
use std::io::{self, Write};

pub(crate) fn validate(links: &Hyperlinks) -> Result<()> {
    for (address, link) in links.iter() {
        if link.target.is_some() && !link.external {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Internal-package hyperlink graph editing remains unimplemented",
            )
            .with_cell(address));
        }
        for text in [&link.target, &link.location, &link.display, &link.tooltip]
            .into_iter()
            .flatten()
        {
            validate_xml_text(text).map_err(|e| e.with_cell(address))?;
        }
    }
    Ok(())
}
pub(crate) fn write_links(
    output: &mut impl Write,
    links: &Hyperlinks,
    uri: Option<&str>,
) -> io::Result<()> {
    if links.is_empty() {
        return Ok(());
    }
    output.write_all(b"<hyperlinks")?;
    if let Some(uri) = uri {
        write_attribute(output, "xmlns", uri)?;
    }
    write_attribute(output, "xmlns:r", OFFICE_REL_URI)?;
    output.write_all(b">")?;
    for (index, (address, link)) in links.iter().enumerate() {
        output.write_all(b"<hyperlink")?;
        write_attribute(output, "ref", &address.to_string())?;
        for (name, value) in [
            ("location", &link.location),
            ("display", &link.display),
            ("tooltip", &link.tooltip),
        ] {
            if let Some(value) = value {
                write_attribute(output, name, value)?;
            }
        }
        if link.target.is_some() {
            write_attribute(output, "r:id", &format!("rId{}", index + 1))?;
        }
        output.write_all(b"/>")?;
    }
    output.write_all(b"</hyperlinks>")
}
pub(crate) fn relationships(links: &Hyperlinks, maximum: usize) -> Result<Option<Vec<u8>>> {
    validate(links)?;
    if !links.iter().any(|(_, link)| link.target.is_some()) {
        return Ok(None);
    }
    let mut output = RowBuffer {
        data: Vec::new(),
        maximum,
    };
    (|| -> io::Result<()> {
        output.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">")?;
        for (index, (_, link)) in links.iter().enumerate() {
            if let Some(target) = &link.target {
                output.write_all(b"<Relationship")?;
                write_attribute(&mut output, "Id", &format!("rId{}", index + 1))?;
                write_attribute(&mut output, "Type", &format!("{OFFICE_REL_URI}/hyperlink"))?;
                write_attribute(&mut output, "Target", target)?;
                write_attribute(&mut output, "TargetMode", "External")?;
                output.write_all(b"/>")?;
            }
        }
        output.write_all(b"</Relationships>")
    })().map_err(|cause| Error::caused_by(
        if cause.kind() == io::ErrorKind::FileTooLarge { ErrorKind::MemoryBudgetExceeded } else { ErrorKind::Io },
        "Cannot encode bounded hyperlink relationships", cause))?;
    Ok(Some(output.data))
}
