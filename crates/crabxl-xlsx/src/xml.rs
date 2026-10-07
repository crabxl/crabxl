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
    pub office_relationship_attribute: Option<usize>,
    pub depth: usize,
}

#[derive(Clone, Copy)]
enum DefaultScope {
    Spreadsheet,
    StrictSpreadsheet,
    Relationships,
    ContentTypes,
    Drawing,
    Other,
}
impl DefaultScope {
    const OTHER: Self = Self::Other;
    fn scope(self) -> Scope {
        match self {
            Self::Spreadsheet | Self::StrictSpreadsheet => Scope::Spreadsheet,
            Self::Relationships => Scope::Relationships,
            Self::ContentTypes => Scope::ContentTypes,
            Self::Drawing => Scope::Drawing,
            Self::Other => Scope::Other,
        }
    }
    fn spreadsheet_uri(self) -> Option<&'static str> {
        match self {
            Self::Spreadsheet => Some(MAIN_URI),
            Self::StrictSpreadsheet => Some(STRICT_MAIN_URI),
            _ => None,
        }
    }
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
    let scope = match namespace {
        ResolveResult::Bound(ns) if ns.as_ref().as_bytes() == MAIN => DefaultScope::Spreadsheet,
        ResolveResult::Bound(ns) if ns.as_ref().as_bytes() == STRICT_MAIN => {
            DefaultScope::StrictSpreadsheet
        }
        ResolveResult::Bound(ns)
            if ns.as_ref().as_bytes()
                == b"http://schemas.openxmlformats.org/package/2006/relationships" =>
        {
            DefaultScope::Relationships
        }
        ResolveResult::Bound(ns)
            if ns.as_ref().as_bytes()
                == b"http://schemas.openxmlformats.org/package/2006/content-types" =>
        {
            DefaultScope::ContentTypes
        }
        ResolveResult::Bound(ns)
            if matches!(
                ns.as_ref().as_bytes(),
                b"http://schemas.openxmlformats.org/drawingml/2006/main"
                    | b"http://purl.oclc.org/ooxml/drawingml/main"
            ) =>
        {
            DefaultScope::Drawing
        }
        ResolveResult::Unknown(_) => {
            return Err(Error::new(
                ErrorKind::Xml,
                "Undeclared XML namespace prefix",
            ));
        }
        _ => DefaultScope::Other,
    };
    Ok(scope)
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

    /// Decode a complete balanced scalar cell already present in the input
    /// buffer. Unrecognized XML remains untouched for the normal event reader.
    pub(crate) fn buffered_scalar<T>(
        &mut self,
        decode: impl FnOnce(&str, &str) -> Result<Option<T>>,
    ) -> Result<Option<T>> {
        self.buffered_element(3, 5, scalar_value_token, decode)
    }

    pub(crate) fn buffered_plain_shared_text<T>(
        &mut self,
        decode: impl FnOnce(&str) -> Result<Option<T>>,
    ) -> Result<Option<T>> {
        self.buffered_element(1, 3, plain_shared_text_token, |_, text| decode(text))
    }

    #[inline(always)]
    fn buffered_element<T>(
        &mut self,
        depth: usize,
        maximum_depth: usize,
        recognize: impl FnOnce(&[u8], usize, usize) -> Option<(&str, &str, usize)>,
        decode: impl FnOnce(&str, &str) -> Result<Option<T>>,
    ) -> Result<Option<T>> {
        self.namespaces.before_event();
        // quick-xml retains the lexical empty start token in read_event_into's
        // buffer while its expanded end event is pending. Never bypass that end.
        if self.depth != depth
            || self.namespaces.default.scope() != Scope::Spreadsheet
            || self.buffer.ends_with(b"/>")
            || self.limits.max_xml_depth < maximum_depth
        {
            return Ok(None);
        }
        let maximum = self.limits.max_xml_event_bytes;
        self.reader.get_mut().event_remaining = maximum.saturating_mul(5).min(1024);
        let bytes = self.reader.get_mut().fill_buf().map_err(|cause| {
            let limited = cause
                .get_ref()
                .is_some_and(|source| source.is::<BudgetExceeded>());
            Error::caused_by(
                if limited {
                    ErrorKind::LimitExceeded
                } else {
                    ErrorKind::Xml
                },
                "Cannot inspect buffered XML cell",
                cause,
            )
            .with_part(self.part.clone())
        })?;
        let Some((header, value, consumed)) = recognize(bytes, maximum, self.limits.max_cell_bytes)
        else {
            return Ok(None);
        };
        let value = decode(header, value)?;
        if value.is_some() {
            // Reader::stream preserves quick-xml offsets. The recognized XML
            // pair is balanced, unprefixed and carries no namespace declaration.
            self.reader.stream().consume(consumed);
        }
        Ok(value)
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
        let default = self
            .namespaces
            .event(&event)
            .map_err(|error| error.with_part(self.part.clone()))?;
        let scope = default.scope();
        let spreadsheet_uri = default.spreadsheet_uri();
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
        let mut office_relationship_attribute = None;
        if scope == Scope::Spreadsheet
            && let Event::Start(e) = &event
            && matches!(
                e.local_name().as_ref().as_bytes(),
                b"sheet" | b"pageSetup" | b"hyperlink"
            )
        {
            for (index, attribute) in e.attributes().enumerate() {
                let attribute = attribute.map_err(|e| {
                    Error::caused_by(ErrorKind::Xml, "Invalid XML attribute", e)
                        .with_part(self.part.clone())
                })?;
                let (namespace, name) = self.namespaces.resolver.resolve_attribute(attribute.key);
                if name.as_ref() == "id"
                    && matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref().as_bytes() == OFFICE_REL || ns.as_ref().as_bytes() == STRICT_OFFICE_REL)
                {
                    if office_relationship.is_some() {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "Duplicate expanded relationship identity",
                        )
                        .with_part(self.part.clone()));
                    }
                    office_relationship_attribute = Some(index);
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
            office_relationship_attribute,
            depth: self.depth,
        })
    }
}

fn plain_shared_text_token(
    bytes: &[u8],
    maximum: usize,
    cell_maximum: usize,
) -> Option<(&str, &str, usize)> {
    if maximum < 5 || !bytes.starts_with(b"<si><t>") {
        return None;
    }
    let length = bytes.get(7..)?.iter().position(|byte| *byte == b'<')?;
    let value = bytes.get(7..7 + length)?;
    if length.saturating_add(1) > maximum
        || length > cell_maximum
        || !bytes.get(7 + length..)?.starts_with(b"</t></si>")
        // Entities and XML line-ending normalization retain the event path.
        || !value.is_ascii()
        || value.iter().any(|byte| matches!(byte, b'&' | b'\r'))
    {
        return None;
    }
    Some(("", std::str::from_utf8(value).ok()?, 7 + length + 9))
}

fn scalar_value_token(
    bytes: &[u8],
    maximum: usize,
    cell_maximum: usize,
) -> Option<(&str, &str, usize)> {
    if maximum < 4 || !(bytes.starts_with(b"<c ") || bytes.starts_with(b"<c>")) {
        return None;
    }
    let end = bytes.iter().position(|byte| *byte == b'>')?;
    if end + 1 > maximum || !bytes.get(end + 1..)?.starts_with(b"<v>") {
        return None;
    }
    let start = end + 4;
    let length = bytes.get(start..)?.iter().position(|byte| *byte == b'<')?;
    if length.saturating_add(1) > maximum
        || length > cell_maximum
        || !bytes.get(start + length..)?.starts_with(b"</v></c>")
    {
        return None;
    }
    let header = bytes.get(1..end)?;
    let value = bytes.get(start..start + length)?;
    if !header.is_ascii()
        || !value.iter().all(|byte| {
            byte.is_ascii_digit()
                || byte.is_ascii_whitespace()
                || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E')
        })
    {
        return None;
    }
    Some((
        std::str::from_utf8(header).ok()?,
        std::str::from_utf8(value).ok()?,
        start + length + 8,
    ))
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
