//! Disk-backed hyperlink declarations for sequential row output.
use super::*;
use crabxl_core::{CellAddress, Hyperlink};

const LINKS_START: &[u8] =
    b"<hyperlinks xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">";
const LINKS_END: &[u8] = b"</hyperlinks>";
const RELS_END: &[u8] = b"</Relationships>";

pub(super) struct LinkSpool {
    declarations: BufWriter<NamedTempFile>,
    relationships: BufWriter<NamedTempFile>,
    declaration_bytes: u64,
    relationship_bytes: u64,
    count: u64,
    external: u64,
}
impl LinkSpool {
    pub(super) fn heap_bytes(&self) -> usize {
        self.declarations.capacity()
            + self.relationships.capacity()
            + self.declarations.get_ref().path().as_os_str().len()
            + self.relationships.get_ref().path().as_os_str().len()
    }
    pub(super) fn pending_sheet_bytes(&self) -> u64 {
        self.declaration_bytes + LINKS_START.len() as u64 + LINKS_END.len() as u64
    }
    pub(super) fn pending_bytes(&self) -> u64 {
        self.pending_sheet_bytes() + RELS_END.len() as u64
    }
    pub(super) fn into_files(self) -> [NamedTempFile; 2] {
        let (declarations, _) = self.declarations.into_parts();
        let (relationships, _) = self.relationships.into_parts();
        [declarations, relationships]
    }
}

impl WorkbookWriter {
    /// Spool a validated sparse row and its point hyperlinks. Link declarations
    /// and relationships use owned disk files; RAM does not grow with link count.
    /// Feature footers and existing point declarations share the same output owner.
    /// Rejection does not partly commit the requested row or its link records;
    /// pending dimension-only rows follow normal sequential emission.
    /// an I/O failure poisons the writer for explicit abort/cleanup.
    pub fn write_row_with_hyperlinks(
        &mut self,
        row: &Row,
        links: &[(CellAddress, Hyperlink)],
    ) -> Result<()> {
        if links.is_empty() {
            return self.write_row(row);
        }
        self.ensure_open()?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.live_events.is_some() {
            return Err(state(
                "Snapshot and live hyperlink rows cannot share a sheet",
            ));
        }
        let prefix = relationship_prefix(active.relationships.as_deref())?;
        footer_split(active.footer.as_deref().unwrap_or(FOOTER))?;
        if links.len() > self.options.max_row_cells {
            return Err(limit("Row hyperlink count exceeds configured limit"));
        }
        let count = active.link_spool.as_ref().map_or(0, |spool| spool.count);
        let end = count
            .checked_add(links.len() as u64)
            .ok_or_else(|| limit("Hyperlink identity space exhausted"))?;
        for (address, link) in links {
            if address.row != row.index {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Hyperlink anchor must belong to the appended row",
                )
                .with_cell(*address));
            }
            crate::hyperlinks::validate_link(*address, link)?;
        }
        let new = active.link_spool.is_none();
        let available = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes())
            .saturating_sub(self.style_memory_bytes());
        let buffers = if new {
            self.options.buffer_bytes.saturating_mul(2)
        } else {
            0
        };
        let mut encoded = RowBuffer {
            data: Vec::new(),
            maximum: available
                .saturating_sub(buffers)
                .min(self.options.max_row_bytes),
        };
        let mut external = 0_u64;
        (|| -> io::Result<()> {
            for (index, (address, link)) in links.iter().enumerate() {
                let id = format!("{prefix}{}", count + index as u64 + 1);
                crate::hyperlinks::write_link(&mut encoded, *address, link, Some(&id))?;
            }
            Ok(())
        })()
        .map_err(link_encoding_error)?;
        let split = encoded.data.len();
        (|| -> io::Result<()> {
            if new {
                encoded.write_all(crate::hyperlinks::RELATIONSHIPS_HEADER)?;
            }
            for (index, (_, link)) in links.iter().enumerate() {
                if let Some(target) = &link.target {
                    let id = format!("{prefix}{}", count + index as u64 + 1);
                    crate::hyperlinks::write_relationship(&mut encoded, &id, target, false)?;
                    external += 1;
                }
            }
            Ok(())
        })()
        .map_err(link_encoding_error)?;
        let created = if new {
            let declarations = self.link_tempfile()?;
            let relationships = self.link_tempfile()?;
            let bytes = declarations.path().as_os_str().len()
                + relationships.path().as_os_str().len()
                + buffers;
            if bytes.saturating_add(encoded.data.capacity()) > available {
                return Err(limit(
                    "Hyperlink spool buffers and paths exceed metadata allowance",
                ));
            }
            Some(LinkSpool {
                declarations: BufWriter::with_capacity(self.options.buffer_bytes, declarations),
                relationships: BufWriter::with_capacity(self.options.buffer_bytes, relationships),
                declaration_bytes: 0,
                relationship_bytes: 0,
                count: 0,
                external: 0,
            })
        } else {
            None
        };
        self.pending_link_bytes = encoded.data.len() as u64
            + split as u64
            + if new {
                (LINKS_START.len() + LINKS_END.len() + RELS_END.len()) as u64
            } else {
                0
            };
        self.pending_link_sheet_bytes = split as u64
            + if new {
                (LINKS_START.len() + LINKS_END.len()) as u64
            } else {
                0
            };
        self.pending_link_metadata_bytes =
            encoded.data.capacity() + created.as_ref().map_or(0, LinkSpool::heap_bytes);
        let written = self.write_row(row);
        self.pending_link_bytes = 0;
        self.pending_link_sheet_bytes = 0;
        self.pending_link_metadata_bytes = 0;
        written?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        if let Some(spool) = created {
            active.link_spool = Some(spool);
        }
        let spool = active
            .link_spool
            .as_mut()
            .ok_or_else(|| state("Missing hyperlink spools"))?;
        if let Err(cause) = spool
            .declarations
            .write_all(&encoded.data[..split])
            .and_then(|_| spool.relationships.write_all(&encoded.data[split..]))
        {
            self.poisoned = true;
            return Err(io_error("Cannot spool row hyperlink metadata", cause));
        }
        spool.declaration_bytes += split as u64;
        spool.relationship_bytes += (encoded.data.len() - split) as u64;
        spool.count = end;
        spool.external += external;
        self.temporary_bytes += encoded.data.len() as u64;
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        Ok(())
    }

    fn link_tempfile(&self) -> Result<NamedTempFile> {
        match &self.options.temp_directory {
            Some(directory) => NamedTempFile::new_in(directory),
            None => NamedTempFile::new(),
        }
        .map_err(|cause| io_error("Cannot create hyperlink temporary file", cause))
    }

    pub(super) fn finish_hyperlinks(&mut self) -> Result<(bool, Option<NamedTempFile>)> {
        let Some(spool) = self
            .active
            .as_ref()
            .and_then(|sheet| sheet.link_spool.as_ref())
        else {
            return Ok((false, None));
        };
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        let footer_bytes = active.footer.as_ref().map_or(FOOTER.len(), Vec::len) as u64;
        let base_bytes = active
            .relationships
            .as_deref()
            .map(relationship_inner)
            .transpose()?
            .map_or(0, <[u8]>::len) as u64;
        let additional = footer_bytes + spool.pending_bytes() + base_bytes;
        self.check_temp(additional + self.paused_footers())?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active
            .bytes
            .saturating_add(footer_bytes)
            .saturating_add(spool.pending_sheet_bytes())
            > self.options.max_sheet_bytes
        {
            return Err(limit("Writer sheet byte limit exceeded"));
        }
        let mut spool = self
            .active
            .as_mut()
            .and_then(|sheet| sheet.link_spool.take())
            .ok_or_else(|| state("Missing hyperlink spools"))?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        let footer = active.footer.take().unwrap_or_else(|| FOOTER.to_vec());
        let base = active.relationships.take();
        let (at, envelope) = footer_split(&footer)?;
        let result = (|| -> Result<()> {
            self.write_active(&footer[..at])?;
            if envelope {
                self.write_active(LINKS_START)?;
            }
            spool
                .declarations
                .rewind()
                .map_err(|cause| io_error("Cannot rewind hyperlink declarations", cause))?;
            let active = self
                .active
                .as_mut()
                .ok_or_else(|| state("No active worksheet"))?;
            let copied = io::copy(spool.declarations.get_mut(), &mut active.output)
                .map_err(|cause| io_error("Cannot finish hyperlink declarations", cause))?;
            if copied != spool.declaration_bytes {
                return Err(state("Hyperlink spool size changed"));
            }
            active.bytes += copied;
            self.temporary_bytes += copied;
            self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
            if envelope {
                self.write_active(LINKS_END)?;
            }
            self.write_active(&footer[at..])?;
            if let Some(base) = &base {
                let bytes = relationship_inner(base)?;
                spool
                    .relationships
                    .write_all(bytes)
                    .map_err(|cause| io_error("Cannot merge feature relationships", cause))?;
                spool.relationship_bytes += bytes.len() as u64;
                self.temporary_bytes += bytes.len() as u64;
            }
            spool
                .relationships
                .write_all(RELS_END)
                .map_err(|cause| io_error("Cannot finish hyperlink relationships", cause))?;
            spool
                .relationships
                .flush()
                .map_err(|cause| io_error("Cannot flush hyperlink relationships", cause))?;
            self.temporary_bytes += RELS_END.len() as u64;
            self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
            Ok(())
        })();
        if let Err(error) = result {
            self.poisoned = true;
            if let Some(active) = &mut self.active {
                active.link_spool = Some(spool);
            }
            return Err(error);
        }
        let has_external = spool.external > 0 || base.is_some();
        let declaration_bytes = spool.declaration_bytes;
        let relationship_bytes = spool.relationship_bytes + RELS_END.len() as u64;
        let (declarations, _) = spool.declarations.into_parts();
        let (relationship_file, _) = spool.relationships.into_parts();
        let mut first_error = self.retire_link_file(declarations).err();
        let relationships = if has_external && first_error.is_none() {
            Some(relationship_file)
        } else {
            if let Err(error) = self.retire_link_file(relationship_file)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
            None
        };
        if let Some(error) = first_error {
            self.poisoned = true;
            return Err(error);
        }
        self.temporary_bytes = self.temporary_bytes.saturating_sub(declaration_bytes);
        if !has_external {
            self.temporary_bytes = self.temporary_bytes.saturating_sub(relationship_bytes);
        }
        Ok((true, relationships))
    }
    fn retire_link_file(&mut self, file: NamedTempFile) -> Result<()> {
        let path = file.path().to_owned();
        if let Err(cause) = file.close()
            && cause.kind() != io::ErrorKind::NotFound
        {
            self.cleanup_paths.push(path);
            return Err(io_error("Cannot remove hyperlink temporary file", cause));
        }
        Ok(())
    }
}
fn link_encoding_error(cause: io::Error) -> Error {
    Error::caused_by(
        if cause.kind() == io::ErrorKind::FileTooLarge {
            ErrorKind::MemoryBudgetExceeded
        } else {
            ErrorKind::Io
        },
        "Cannot encode bounded row hyperlink metadata",
        cause,
    )
}

// These envelopes are generated by this writer, rather than read from arbitrary packages.
pub(super) fn relationship_inner(bytes: &[u8]) -> Result<&[u8]> {
    let marker = b"<Relationships";
    let start = bytes
        .windows(marker.len())
        .position(|window| window == marker)
        .ok_or_else(|| state("Missing relationship envelope"))?;
    let start = start
        + bytes[start..]
            .iter()
            .position(|byte| *byte == b'>')
            .ok_or_else(|| state("Invalid relationship envelope"))?
        + 1;
    let end = bytes
        .len()
        .checked_sub(RELS_END.len())
        .filter(|end| *end >= start && bytes.ends_with(RELS_END))
        .ok_or_else(|| state("Invalid relationship closing tag"))?;
    Ok(&bytes[start..end])
}
pub(super) fn relationship_prefix(bytes: Option<&[u8]>) -> Result<String> {
    let mut prefix = String::from("rId");
    if let Some(bytes) = bytes {
        let mut xml = crate::xml::XmlStream::new(
            io::Cursor::new(bytes),
            "relationships".into(),
            bytes.len() as u64,
            Default::default(),
        );
        let mut ids = Vec::new();
        loop {
            let frame = xml.next()?;
            match frame.event {
                quick_xml::events::Event::Start(e) if frame.depth == 2 => {
                    ids.push(crate::xml::required_attribute(&e, b"Id")?)
                }
                quick_xml::events::Event::Eof => break,
                _ => {}
            }
        }
        while ids.iter().any(|id| {
            id.strip_prefix(&prefix).is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
        }) {
            prefix.push('_');
        }
    }
    Ok(prefix)
}
pub(super) fn footer_split(footer: &[u8]) -> Result<(usize, bool)> {
    use quick_xml::events::Event;
    use std::io::Read;
    const PREFIX: &[u8] = b"<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>";
    let input = io::Cursor::new(PREFIX).chain(io::Cursor::new(footer));
    let mut xml = crate::xml::XmlStream::new(
        BufReader::new(input),
        "footer".into(),
        (PREFIX.len() + footer.len()) as u64,
        Default::default(),
    );
    let rank = crate::features::sheet::rank(b"hyperlinks");
    loop {
        let position = xml.bytes_consumed();
        let frame = xml.next()?;
        let envelope = match &frame.event {
            Event::Start(e)
                if frame.depth == 2
                    && crate::features::sheet::rank(e.local_name().as_ref().as_bytes()) > rank =>
            {
                Some(true)
            }
            Event::End(e) if frame.depth == 1 && e.local_name().as_ref() == "hyperlinks" => {
                Some(false)
            }
            Event::End(_) if frame.depth == 0 => Some(true),
            Event::Eof => return Err(state("Invalid worksheet footer")),
            _ => None,
        };
        if let Some(envelope) = envelope {
            let at = usize::try_from(position)
                .ok()
                .and_then(|position| position.checked_sub(PREFIX.len()))
                .filter(|at| *at <= footer.len())
                .ok_or_else(|| state("Invalid hyperlink insertion position"))?;
            return Ok((at, envelope));
        }
    }
}
