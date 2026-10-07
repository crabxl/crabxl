// SPDX-License-Identifier: MIT
// External hyperlink relationship ordering adapted from umya-spreadsheet,
// Copyright (c) 2020 MathNya. Bounded codecs and full optional fields are CrabXL.
//! Point hyperlink metadata and owned-package relationship encoding.
pub(crate) mod rewrite;
mod scan;
pub(crate) mod source;
pub(crate) use scan::Capture;

use crate::{
    encode::{RowBuffer, validate_xml_text, write_attribute},
    xml::OFFICE_REL_URI,
};
use crabxl_core::{Error, ErrorKind, Hyperlinks, Result};
use std::{
    borrow::Cow,
    io::{self, Write},
};

pub(crate) fn validate(links: &Hyperlinks) -> Result<()> {
    for (address, link) in links.iter() {
        validate_link(address, link)?;
    }
    Ok(())
}
pub(crate) fn validate_link(
    address: crabxl_core::CellAddress,
    link: &crabxl_core::Hyperlink,
) -> Result<()> {
    link.validate_reference()
        .map_err(|error| error.with_cell(address))?;
    if link.target.is_none() && link.relationship_id.is_some() {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "Unresolved source hyperlink targets require relationship resolution",
        )
        .with_cell(address));
    }
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
    Ok(())
}

pub(crate) fn write_links(
    output: &mut impl Write,
    links: &Hyperlinks,
    uri: Option<&str>,
) -> io::Result<()> {
    write_links_with_ids(output, links, uri, |index| {
        Some(Cow::Owned(format!("rId{}", index + 1)))
    })
}
pub(crate) fn write_links_with_ids<'a>(
    output: &mut impl Write,
    links: &Hyperlinks,
    uri: Option<&str>,
    mut identity: impl FnMut(usize) -> Option<Cow<'a, str>>,
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
        let id = link.target.as_ref().and_then(|_| identity(index));
        write_link(output, address, link, id.as_deref())?;
    }
    output.write_all(b"</hyperlinks>")
}
pub(crate) fn write_link(
    output: &mut impl Write,
    address: crabxl_core::CellAddress,
    link: &crabxl_core::Hyperlink,
    identity: Option<&str>,
) -> io::Result<()> {
    output.write_all(b"<hyperlink")?;
    if let Some(reference) = &link.reference {
        write_attribute(output, "ref", reference)?;
    } else {
        write_attribute(output, "ref", &address.to_string())?;
    }
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
        let identity = identity.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing encoded hyperlink identity",
            )
        })?;
        write_attribute(output, "r:id", identity)?;
    }
    output.write_all(b"/>")
}

pub(crate) const RELATIONSHIPS_HEADER: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">";

pub(crate) fn write_relationship(
    output: &mut impl Write,
    identity: &str,
    target: &str,
    explicit_namespace: bool,
) -> io::Result<()> {
    output.write_all(b"<Relationship")?;
    if explicit_namespace {
        write_attribute(
            output,
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        )?;
    }
    write_attribute(output, "Id", identity)?;
    write_attribute(output, "Type", &format!("{OFFICE_REL_URI}/hyperlink"))?;
    write_attribute(output, "Target", target)?;
    output.write_all(b" TargetMode=\"External\"/>")
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
        output.write_all(RELATIONSHIPS_HEADER)?;
        for (index, (_, link)) in links.iter().enumerate() {
            if let Some(target) = &link.target {
                write_relationship(&mut output, &format!("rId{}", index + 1), target, false)?;
            }
        }
        output.write_all(b"</Relationships>")
    })()
    .map_err(|cause| {
        Error::caused_by(
            if cause.kind() == io::ErrorKind::FileTooLarge {
                ErrorKind::MemoryBudgetExceeded
            } else {
                ErrorKind::Io
            },
            "Cannot encode bounded hyperlink relationships",
            cause,
        )
    })?;
    Ok(Some(output.data))
}
