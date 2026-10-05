// SPDX-License-Identifier: MIT
// XML reader configuration adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Namespace event ownership adapted from quick-xml, Copyright (c) 2016 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

use crabxl_core::{Error, ErrorKind, ResourceLimits, Result};
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
    name::{NamespaceError, NamespaceResolver, QName, ResolveResult},
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
pub(crate) const STRICT_OFFICE_REL_URI: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships";
const STRICT_OFFICE_REL: &[u8] = STRICT_OFFICE_REL_URI.as_bytes();

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Spreadsheet,
    Relationships,
    ContentTypes,
    Drawing,
    Other,
}

pub(crate) struct Frame<'a> {
    pub scope: Scope,
    pub spreadsheet_uri: Option<&'static str>,
    pub event: Event<'a>,
    pub office_relationship: Option<String>,
    pub depth: usize,
}

#[derive(Clone, Copy)]
struct DefaultScope {
    scope: Scope,
    spreadsheet_uri: Option<&'static str>,
}
impl DefaultScope {
    const OTHER: Self = Self {
        scope: Scope::Other,
        spreadsheet_uri: None,
    };
}
struct NamespaceSnapshot {
    level: u16,
    previous: DefaultScope,
}

/// Keep quick-xml's resolver authoritative, caching only semantic default scope.
/// No attribute can declare a namespace without a lowercase `x` in its key.
struct StreamNamespaces {
    resolver: NamespaceResolver,
    default: DefaultScope,
    snapshots: Vec<NamespaceSnapshot>,
    pending_pop: bool,
    element_level: u16,
}
impl StreamNamespaces {
    fn new() -> Self {
        Self {
            resolver: NamespaceResolver::default(),
            default: DefaultScope::OTHER,
            snapshots: Vec::new(),
            pending_pop: false,
            element_level: 0,
        }
    }
    fn before_event(&mut self) {
        if self.pending_pop {
            if self
                .snapshots
                .last()
                .is_some_and(|v| v.level == self.element_level)
            {
                self.default = self
                    .snapshots
                    .pop()
                    .map_or(DefaultScope::OTHER, |v| v.previous);
                self.resolver.pop();
            }
            self.element_level = self.element_level.saturating_sub(1);
            self.pending_pop = false;
        }
    }
    fn event(&mut self, event: &Event<'_>) -> Result<DefaultScope> {
        let name = match event {
            Event::Start(start) | Event::Empty(start) => {
                self.element_level = self.element_level.checked_add(1).ok_or_else(|| {
                    Error::caused_by(
                        ErrorKind::Xml,
                        "Invalid XML namespace nesting",
                        NamespaceError::TooDeeplyNested(u16::MAX as usize),
                    )
                })?;
                let mut declaration = false;
                if start.attributes_raw().as_bytes().contains(&b'x') {
                    for attribute in start.attributes().with_checks(false) {
                        let Ok(attribute) = attribute else { break };
                        if attribute.key.as_namespace_binding().is_some() {
                            declaration = true;
                            break;
                        }
                    }
                }
                if declaration {
                    self.snapshots.try_reserve_exact(1).map_err(|cause| {
                        Error::caused_by(
                            ErrorKind::LimitExceeded,
                            "Cannot allocate namespace scope cache",
                            cause,
                        )
                    })?;
                    let previous = self.default;
                    self.resolver.push(start).map_err(|cause| {
                        Error::caused_by(ErrorKind::Xml, "Invalid XML namespace declaration", cause)
                    })?;
                    self.default = namespace_scope(self.resolver.resolve_element(QName("n")).0)?;
                    self.snapshots.push(NamespaceSnapshot {
                        level: self.element_level,
                        previous,
                    });
                }
                self.pending_pop = matches!(event, Event::Empty(_));
                start.name()
            }
            Event::End(end) => {
                self.pending_pop = true;
                end.name()
            }
            _ => return Ok(DefaultScope::OTHER),
        };
        if !name.as_ref().as_bytes().contains(&b':') {
            Ok(self.default)
        } else {
            namespace_scope(self.resolver.resolve_element(name).0)
        }
    }
}

fn namespace_scope(namespace: ResolveResult<'_>) -> Result<DefaultScope> {
    let (scope, spreadsheet_uri) = match namespace {
        ResolveResult::Bound(ns) if ns.as_ref().as_bytes() == MAIN => {
            (Scope::Spreadsheet, Some(MAIN_URI))
        }
        ResolveResult::Bound(ns) if ns.as_ref().as_bytes() == STRICT_MAIN => {
            (Scope::Spreadsheet, Some(STRICT_MAIN_URI))
        }
        ResolveResult::Bound(ns)
            if ns.as_ref().as_bytes()
                == b"http://schemas.openxmlformats.org/package/2006/relationships" =>
        {
            (Scope::Relationships, None)
        }
        ResolveResult::Bound(ns)
            if ns.as_ref().as_bytes()
                == b"http://schemas.openxmlformats.org/package/2006/content-types" =>
        {
            (Scope::ContentTypes, None)
        }
        ResolveResult::Bound(ns)
            if matches!(
                ns.as_ref().as_bytes(),
                b"http://schemas.openxmlformats.org/drawingml/2006/main"
                    | b"http://purl.oclc.org/ooxml/drawingml/main"
            ) =>
        {
            (Scope::Drawing, None)
        }
        ResolveResult::Unknown(_) => {
            return Err(Error::new(
                ErrorKind::Xml,
                "Undeclared XML namespace prefix",
            ));
        }
        _ => (Scope::Other, None),
    };
    Ok(DefaultScope {
        scope,
        spreadsheet_uri,
    })
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
    reader: Reader<BudgetInput<B>>,
    namespaces: StreamNamespaces,
    buffer: Vec<u8>,
    limits: ResourceLimits,
    part: String,
    byte_limit: u64,
    depth: usize,
    root_seen: bool,
}
impl<B: BufRead> XmlStream<B> {
    pub fn new(input: B, part: String, byte_limit: u64, limits: ResourceLimits) -> Self {
        let mut reader = Reader::from_reader(BudgetInput {
            inner: input,
            event_remaining: limits.max_xml_event_bytes,
            part_remaining: byte_limit,
        });
        reader.config_mut().expand_empty_elements = true;
        // Keep end-name and comment validation enabled; text whitespace is data.
        reader.config_mut().trim_text(false);
        Self {
            reader,
            namespaces: StreamNamespaces::new(),
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

    #[inline(always)]
    pub fn next(&mut self) -> Result<Frame<'_>> {
        self.buffer.clear();
        self.namespaces.before_event();
        self.reader.get_mut().event_remaining = self.limits.max_xml_event_bytes;
        let event = self.reader.read_event_into(&mut self.buffer).map_err(|cause| {
            let limited = matches!(&cause, quick_xml::Error::Io(e) if e.get_ref().is_some_and(|source| source.is::<BudgetExceeded>()));
            Error::caused_by(if limited { ErrorKind::LimitExceeded } else { ErrorKind::Xml }, "Cannot parse XML", cause).with_part(self.part.clone())
        })?;
        let DefaultScope {
            scope,
            spreadsheet_uri,
        } = self
            .namespaces
            .event(&event)
            .map_err(|error| error.with_part(self.part.clone()))?;
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
            Event::Text(t)
                if self.depth == 0
                    && !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
            {
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
        let mut office_relationship = None;
        if scope == Scope::Spreadsheet
            && let Event::Start(e) = &event
            && matches!(e.local_name().as_ref().as_bytes(), b"sheet" | b"pageSetup")
        {
            for attribute in e.attributes() {
                let attribute = attribute.map_err(|e| {
                    Error::caused_by(ErrorKind::Xml, "Invalid XML attribute", e)
                        .with_part(self.part.clone())
                })?;
                let (namespace, name) = self.namespaces.resolver.resolve_attribute(attribute.key);
                if name.as_ref() == "id"
                    && matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref().as_bytes() == OFFICE_REL || ns.as_ref().as_bytes() == STRICT_OFFICE_REL)
                {
                    office_relationship = Some(
                        attribute
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|e| {
                                Error::caused_by(ErrorKind::Xml, "Invalid relationship ID", e)
                                    .with_part(self.part.clone())
                            })?
                            .into_owned(),
                    );
                }
            }
        }
        Ok(Frame {
            scope,
            spreadsheet_uri,
            event,
            office_relationship,
            depth: self.depth,
        })
    }
}

pub(crate) fn attribute(e: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>> {
    let value = e
        .try_get_attribute(
            std::str::from_utf8(name).map_err(|cause| {
                Error::caused_by(ErrorKind::Xml, "Invalid attribute name", cause)
            })?,
        )
        .map_err(|cause| Error::caused_by(ErrorKind::Xml, "Invalid XML attribute", cause))?;
    value
        .map(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|v| v.into_owned())
                .map_err(|cause| {
                    Error::caused_by(ErrorKind::Xml, "Invalid XML attribute value", cause)
                })
        })
        .transpose()
}

pub(crate) fn required_attribute(e: &BytesStart<'_>, name: &[u8]) -> Result<String> {
    attribute(e, name)?.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Missing XML attribute {}", String::from_utf8_lossy(name)),
        )
    })
}

/// Append one XML character-data event into a bounded reusable value buffer.
pub(crate) fn append_xml_text(
    output: &mut String,
    event: &Event<'_>,
    maximum: usize,
) -> Result<()> {
    let append = |output: &mut String, text: &str| -> Result<()> {
        if output.len().saturating_add(text.len()) > maximum {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Cell value byte limit exceeded",
            ));
        }
        output.try_reserve_exact(text.len()).map_err(|e| {
            Error::caused_by(
                ErrorKind::LimitExceeded,
                "Cannot allocate cell value buffer",
                e,
            )
        })?;
        output.push_str(text);
        Ok(())
    };
    match event {
        Event::Text(t) => append(output, &t.xml10_content()),
        Event::CData(t) => append(output, &t.xml10_content()),
        Event::GeneralRef(e) => {
            let entity = e.as_ref();
            if let Some(text) = quick_xml::escape::resolve_xml_entity(entity) {
                append(output, text)
            } else if let Some(character) = e
                .resolve_char_ref()
                .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid XML entity", e))?
            {
                let mut bytes = [0u8; 4];
                append(output, character.encode_utf8(&mut bytes))
            } else {
                Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unrecognized XML entity",
                ))
            }
        }
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            "Expected XML character data",
        )),
    }
}
