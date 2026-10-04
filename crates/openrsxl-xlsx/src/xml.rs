// SPDX-License-Identifier: MIT
// XML reader configuration adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

use openrsxl_core::{Error, ErrorKind, ResourceLimits, Result};
use quick_xml::{
    NsReader,
    encoding::Decoder,
    events::{BytesStart, Event},
    name::ResolveResult,
};
use std::{
    fmt,
    io::{self, BufRead, Read},
};

pub(crate) const MAIN_URI: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub(crate) const MAIN: &[u8] = MAIN_URI.as_bytes();
pub(crate) const STRICT_MAIN_URI: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";
pub(crate) const STRICT_MAIN: &[u8] = STRICT_MAIN_URI.as_bytes();
pub(crate) const OFFICE_REL_URI: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const OFFICE_REL: &[u8] = OFFICE_REL_URI.as_bytes();
const STRICT_OFFICE_REL: &[u8] = b"http://purl.oclc.org/ooxml/officeDocument/relationships";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Spreadsheet,
    Relationships,
    ContentTypes,
    Other,
}

pub(crate) struct Frame<'a> {
    pub scope: Scope,
    pub spreadsheet_uri: Option<&'static str>,
    pub event: Event<'a>,
    pub decoder: Decoder,
    pub sheet_relationship: Option<String>,
    pub depth: usize,
}

#[derive(Debug)]
struct BudgetExceeded(&'static str);
impl fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} exceeded", self.0)
    }
}
impl std::error::Error for BudgetExceeded {}

/// Prevent quick-xml from growing its event buffer beyond a configured bound.
struct BudgetInput<B> {
    inner: B,
    event_remaining: usize,
    part_remaining: u64,
}
impl<B: BufRead> BufRead for BudgetInput<B> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let bytes = self.inner.fill_buf()?;
        if bytes.is_empty() {
            return Ok(bytes);
        }
        if self.part_remaining == 0 {
            return Err(io::Error::other(BudgetExceeded("XML part byte limit")));
        }
        if self.event_remaining == 0 {
            return Err(io::Error::other(BudgetExceeded("XML event byte limit")));
        }
        let available = bytes
            .len()
            .min(self.event_remaining)
            .min(self.part_remaining.min(usize::MAX as u64) as usize);
        Ok(&bytes[..available])
    }
    fn consume(&mut self, amount: usize) {
        self.event_remaining -= amount;
        self.part_remaining -= amount as u64;
        self.inner.consume(amount);
    }
}
impl<B: BufRead> Read for BudgetInput<B> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let bytes = self.fill_buf()?;
        let count = output.len().min(bytes.len());
        output[..count].copy_from_slice(&bytes[..count]);
        self.consume(count);
        Ok(count)
    }
}

pub(crate) struct XmlStream<B> {
    reader: NsReader<BudgetInput<B>>,
    buffer: Vec<u8>,
    limits: ResourceLimits,
    part: String,
    byte_limit: u64,
    depth: usize,
    root_seen: bool,
}
impl<B: BufRead> XmlStream<B> {
    pub fn new(input: B, part: String, byte_limit: u64, limits: ResourceLimits) -> Self {
        let mut reader = NsReader::from_reader(BudgetInput {
            inner: input,
            event_remaining: limits.max_xml_event_bytes,
            part_remaining: byte_limit,
        });
        reader.config_mut().expand_empty_elements = true;
        // Keep end-name and comment validation enabled; text whitespace is data.
        reader.config_mut().trim_text(false);
        Self {
            reader,
            buffer: Vec::with_capacity(1024.min(limits.max_xml_event_bytes)),
            limits,
            part,
            byte_limit,
            depth: 0,
            root_seen: false,
        }
    }

    pub fn part(&self) -> &str {
        &self.part
    }
    pub(crate) fn bytes_consumed(&self) -> u64 {
        self.byte_limit - self.reader.get_ref().part_remaining
    }

    pub fn next(&mut self) -> Result<Frame<'_>> {
        self.buffer.clear();
        self.reader.get_mut().event_remaining = self.limits.max_xml_event_bytes;
        let decoder = self.reader.decoder();
        let (namespace, event) = self.reader.read_resolved_event_into(&mut self.buffer).map_err(|cause| {
            let limited = matches!(&cause, quick_xml::Error::Io(e) if e.get_ref().is_some_and(|source| source.is::<BudgetExceeded>()));
            Error::caused_by(if limited { ErrorKind::LimitExceeded } else { ErrorKind::Xml }, "Cannot parse XML", cause).with_part(self.part.clone())
        })?;
        let spreadsheet_uri = match &namespace {
            ResolveResult::Bound(ns) if ns.as_ref() == MAIN => Some(MAIN_URI),
            ResolveResult::Bound(ns) if ns.as_ref() == STRICT_MAIN => Some(STRICT_MAIN_URI),
            _ => None,
        };
        let scope = match namespace {
            ResolveResult::Bound(ns) if ns.as_ref() == MAIN || ns.as_ref() == STRICT_MAIN => {
                Scope::Spreadsheet
            }
            ResolveResult::Bound(ns)
                if ns.as_ref()
                    == b"http://schemas.openxmlformats.org/package/2006/relationships" =>
            {
                Scope::Relationships
            }
            ResolveResult::Bound(ns)
                if ns.as_ref()
                    == b"http://schemas.openxmlformats.org/package/2006/content-types" =>
            {
                Scope::ContentTypes
            }
            ResolveResult::Unknown(_) => {
                return Err(
                    Error::new(ErrorKind::Xml, "Undeclared XML namespace prefix")
                        .with_part(self.part.clone()),
                );
            }
            _ => Scope::Other,
        };
        match &event {
            Event::Start(_) => {
                if self.depth == 0 && self.root_seen {
                    return Err(Error::new(ErrorKind::Xml, "Multiple XML roots")
                        .with_part(self.part.clone()));
                }
                self.root_seen = true;
                self.depth += 1;
                if self.depth > self.limits.max_xml_depth {
                    return Err(
                        Error::new(ErrorKind::LimitExceeded, "XML nesting limit exceeded")
                            .with_part(self.part.clone()),
                    );
                }
            }
            Event::End(_) => {
                self.depth = self.depth.checked_sub(1).ok_or_else(|| {
                    Error::new(ErrorKind::Xml, "Unexpected XML end tag")
                        .with_part(self.part.clone())
                })?;
            }
            Event::DocType(_) => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "XML document types are not supported",
                )
                .with_part(self.part.clone()));
            }
            Event::Decl(_) if self.root_seen => {
                return Err(
                    Error::new(ErrorKind::Xml, "XML declaration inside document")
                        .with_part(self.part.clone()),
                );
            }
            Event::Text(t) if self.depth == 0 && !t.iter().all(u8::is_ascii_whitespace) => {
                return Err(Error::new(ErrorKind::Xml, "Text outside XML root")
                    .with_part(self.part.clone()));
            }
            Event::CData(_) | Event::GeneralRef(_) if self.depth == 0 => {
                return Err(Error::new(ErrorKind::Xml, "Content outside XML root")
                    .with_part(self.part.clone()));
            }
            Event::Eof if self.depth != 0 || !self.root_seen => {
                return Err(Error::new(ErrorKind::Xml, "Incomplete XML document")
                    .with_part(self.part.clone()));
            }
            _ => {}
        }
        let mut sheet_relationship = None;
        if scope == Scope::Spreadsheet {
            if let Event::Start(e) = &event {
                if e.local_name().as_ref() == b"sheet" {
                    for attribute in e.attributes() {
                        let attribute = attribute.map_err(|e| {
                            Error::caused_by(ErrorKind::Xml, "Invalid XML attribute", e)
                                .with_part(self.part.clone())
                        })?;
                        let (namespace, name) =
                            self.reader.resolver().resolve_attribute(attribute.key);
                        if name.as_ref() == b"id"
                            && matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == OFFICE_REL || ns.as_ref() == STRICT_OFFICE_REL)
                        {
                            sheet_relationship = Some(
                                attribute
                                    .decoded_and_normalized_value(
                                        quick_xml::XmlVersion::Implicit1_0,
                                        decoder,
                                    )
                                    .map_err(|e| {
                                        Error::caused_by(
                                            ErrorKind::Xml,
                                            "Invalid relationship ID",
                                            e,
                                        )
                                        .with_part(self.part.clone())
                                    })?
                                    .into_owned(),
                            );
                        }
                    }
                }
            }
        }
        Ok(Frame {
            scope,
            spreadsheet_uri,
            event,
            decoder,
            sheet_relationship,
            depth: self.depth,
        })
    }
}

pub(crate) fn attribute(
    e: &BytesStart<'_>,
    name: &[u8],
    decoder: Decoder,
) -> Result<Option<String>> {
    let value = e
        .try_get_attribute(name)
        .map_err(|cause| Error::caused_by(ErrorKind::Xml, "Invalid XML attribute", cause))?;
    value
        .map(|a| {
            a.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
                .map(|v| v.into_owned())
                .map_err(|cause| {
                    Error::caused_by(ErrorKind::Xml, "Invalid XML attribute value", cause)
                })
        })
        .transpose()
}

pub(crate) fn required_attribute(
    e: &BytesStart<'_>,
    name: &[u8],
    decoder: Decoder,
) -> Result<String> {
    attribute(e, name, decoder)?.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Missing XML attribute {}", String::from_utf8_lossy(name)),
        )
    })
}
