//! Live hyperlink aliases evaluated at packaging with bounded disk storage.
use super::*;
use crabxl_core::{CellAddress, Hyperlink};
mod store;
pub(super) use store::{Events, Store};

impl WorkbookWriter {
    /// Register a mutable writer-local hyperlink identity. Its payload is stored
    /// on disk, independently of the number of rows referring to the identity.
    pub fn register_hyperlink_group(&mut self, link: &Hyperlink) -> Result<u64> {
        let group = self.live_links.as_ref().map_or(0, |store| store.count);
        self.store_hyperlink_group(group, link, true)?;
        Ok(group)
    }
    /// Change a registered alias, including records in already closed sheets.
    /// Serialized cell values remain snapshots of their original appended rows.
    pub fn update_hyperlink_group(&mut self, group: u64, link: &Hyperlink) -> Result<()> {
        self.store_hyperlink_group(group, link, false)
    }
    /// Read the current owned metadata for a registered alias, using the same
    /// bounded decoder as packaging. Primarily useful for transactional adapters.
    pub fn hyperlink_group(&mut self, group: u64) -> Result<Hyperlink> {
        self.ensure_open()?;
        let maximum = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes())
            .saturating_sub(self.style_memory_bytes());
        self.live_links
            .as_mut()
            .ok_or_else(|| state("Unknown hyperlink group"))?
            .read(group, maximum)
    }
    fn store_hyperlink_group(&mut self, group: u64, link: &Hyperlink, new: bool) -> Result<()> {
        self.ensure_open()?;
        crate::hyperlinks::validate_link(CellAddress::new(0, 0)?, link)?;
        if !new {
            self.live_links
                .as_ref()
                .ok_or_else(|| state("Unknown hyperlink group"))?
                .validate(group)?;
        }
        let available = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes())
            .saturating_sub(self.style_memory_bytes());
        let payload = store::encode(link, available.min(self.options.max_row_bytes))?;
        let added = payload.len() as u64 + if new { 16 } else { 0 };
        self.check_temp(added + self.paused_footers())?;
        let created = if self.live_links.is_none() {
            let index = self.live_link_tempfile()?;
            let values = self.live_link_tempfile()?;
            let store = Store {
                index,
                values,
                count: 0,
                bytes: 0,
            };
            if store.heap_bytes().saturating_add(payload.capacity()) > available {
                return Err(limit(
                    "Live hyperlink group paths exceed metadata allowance",
                ));
            }
            Some(store)
        } else {
            None
        };
        if let Some(store) = created {
            self.live_links = Some(store);
        }
        let result = self
            .live_links
            .as_mut()
            .ok_or_else(|| state("Missing hyperlink store"))?
            .update(group, &payload, new);
        if result.is_err() {
            self.poisoned = true;
        }
        result?;
        self.temporary_bytes += added;
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        Ok(())
    }
    fn live_link_tempfile(&self) -> Result<NamedTempFile> {
        match &self.options.temp_directory {
            Some(directory) => NamedTempFile::new_in(directory),
            None => NamedTempFile::new(),
        }
        .map_err(|cause| io_error("Cannot create live hyperlink temporary file", cause))
    }
    /// Append owner events referencing live hyperlink groups. Row cells are
    /// serialized immediately; metadata is resolved only by finish.
    pub fn write_row_with_hyperlink_groups(
        &mut self,
        row: &Row,
        links: &[(CellAddress, u64)],
    ) -> Result<()> {
        if links.is_empty() {
            return self.write_row(row);
        }
        self.ensure_open()?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| state("No active worksheet"))?;
        if active.footer.is_some() || active.relationships.is_some() || active.link_spool.is_some()
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Live hyperlink groups with prebuilt or snapshot metadata remain unimplemented",
            ));
        }
        if links.len() > self.options.max_row_cells {
            return Err(limit("Too many row hyperlinks"));
        }
        let store = self
            .live_links
            .as_ref()
            .ok_or_else(|| state("Unknown hyperlink groups"))?;
        for (address, group) in links {
            if address.row != row.index {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Hyperlink owner must belong to appended row",
                )
                .with_cell(*address));
            }
            store.validate(*group)?;
        }
        let events = if active.live_events.is_none() {
            let available = self
                .options
                .max_metadata_bytes
                .saturating_sub(self.catalog_bytes())
                .saturating_sub(self.style_memory_bytes());
            let file = self.live_link_tempfile()?;
            if self
                .options
                .buffer_bytes
                .saturating_add(file.path().as_os_str().len())
                > available
            {
                return Err(limit(
                    "Live hyperlink event buffer exceeds metadata allowance",
                ));
            }
            Some(Events {
                output: BufWriter::with_capacity(self.options.buffer_bytes, file),
                count: 0,
            })
        } else {
            None
        };
        self.pending_link_bytes = (links.len() as u64).saturating_mul(16);
        self.pending_link_metadata_bytes = events.as_ref().map_or(0, Events::heap_bytes);
        let result = self.write_row(row);
        self.pending_link_bytes = 0;
        self.pending_link_metadata_bytes = 0;
        result?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| state("No active worksheet"))?;
        if let Some(events) = events {
            active.live_events = Some(events);
        }
        let result = (|| {
            let events = active
                .live_events
                .as_mut()
                .ok_or_else(|| state("Missing hyperlink events"))?;
            for (address, group) in links {
                events.append(*address, *group)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result?;
        self.temporary_bytes += (links.len() as u64) * 16;
        self.stats.peak_temp_bytes = self.stats.peak_temp_bytes.max(self.temporary_bytes);
        Ok(())
    }
}

/// Render one worksheet's deferred declarations or relationships, retaining one
/// bounded decoded payload at a time. Owner order controls output identities.
pub(super) fn write_events(
    output: &mut impl Write,
    events: &mut Events,
    store: &mut Store,
    maximum: usize,
    relationships: bool,
    part_maximum: u64,
    mut identity: impl FnMut(u64, Option<&str>) -> Result<()>,
) -> Result<bool> {
    events.rewind()?;
    let output = &mut BoundedOutput {
        output,
        remaining: part_maximum,
    };
    let mut external = false;
    let header = if relationships {
        crate::hyperlinks::RELATIONSHIPS_HEADER
    } else {
        b"<hyperlinks xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">"
    };
    output
        .write_all(header)
        .map_err(|cause| io_error("Cannot write live hyperlink header", cause))?;
    let mut current = None;
    for index in 0..events.count {
        let (address, group) = events.next()?;
        if current
            .as_ref()
            .is_none_or(|(previous, _)| *previous != group)
        {
            // Drop the previous payload before allocating its replacement.
            drop(current.take());
            current = Some((group, store.read(group, maximum.saturating_sub(64))?));
        }
        let link = &current
            .as_ref()
            .ok_or_else(|| state("Missing decoded hyperlink"))?
            .1;
        let id = format!("rId{}", index + 1);
        let written = if relationships {
            if let Some(target) = &link.target {
                crate::hyperlinks::write_relationship(output, &id, target, false)
            } else {
                Ok(())
            }
        } else {
            identity(group, link.target.as_ref().map(|_| id.as_str()))?;
            crate::hyperlinks::write_link(output, address, link, Some(&id))
        };
        written.map_err(|cause| io_error("Cannot package live hyperlink metadata", cause))?;
        external |= link.target.is_some();
    }
    output
        .write_all(if relationships {
            b"</Relationships>"
        } else {
            b"</hyperlinks>"
        })
        .map_err(|cause| io_error("Cannot finish live hyperlink metadata", cause))?;
    Ok(external)
}

/// Enforce generated part limits without retaining generated XML.
struct BoundedOutput<'a, W> {
    output: &'a mut W,
    remaining: u64,
}
impl<W: Write> Write for BoundedOutput<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Generated hyperlink metadata exceeds part byte limit",
            ));
        }
        let written = self.output.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
