// SPDX-License-Identifier: MIT
// SST event flow adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Storage, budgeting and relationship integration are CrabXL implementations.
// Source provenance: third_party/ports.json.

use crate::{
    memory_allowance,
    xml::{Scope, XmlStream},
};
use crabxl_core::{CellValue, Error, ErrorKind, MemoryPolicy, ResourceLimits, Result};
use quick_xml::events::Event;
use std::{
    fs::File,
    io::{BufRead, BufWriter, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::Arc,
};

/// Placement of decoded shared strings and their index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SharedStringStorage {
    /// Start in memory and spill the data and index when actual usage exceeds budget.
    #[default]
    Auto,
    /// Require the complete table to fit the managed memory allowance.
    Memory,
    /// Use disk for both payloads and index, with a bounded decoded cache.
    Disk,
}

/// Shared-string component limits. Returned rows, archive metadata and dependencies
/// are additional. These settings do not impose a hard process RSS limit.
#[derive(Clone, Debug)]
pub struct SharedStringOptions {
    /// Select adaptive, forced memory or forced disk storage.
    pub storage: SharedStringStorage,
    /// Availability-based or explicit allowance including parser working reserve.
    pub memory_policy: MemoryPolicy,
    /// Maximum retained decoded cache bytes, including slots and payloads.
    pub cache_bytes: usize,
    /// Combined data/index temporary storage ceiling.
    pub max_temp_bytes: u64,
    /// Maximum actual entries; declared uniqueCount is never an allocation request.
    pub max_entries: u64,
    /// Directory for owned temporary files; None uses the system temporary directory.
    pub temp_directory: Option<PathBuf>,
}
impl Default for SharedStringOptions {
    fn default() -> Self {
        Self {
            storage: SharedStringStorage::Auto,
            memory_policy: MemoryPolicy::default(),
            cache_bytes: 8 * 1024 * 1024,
            max_temp_bytes: 4 * 1024 * 1024 * 1024,
            max_entries: 100_000_000,
            temp_directory: None,
        }
    }
}

/// Current shared-string component diagnostics, excluding returned owned values.
#[derive(Clone, Copy, Debug, Default)]
pub struct SharedStringStats {
    /// Selected policy budget including parser working reserve.
    pub budget_bytes: usize,
    /// Bytes available for retained table/cache after parser reserve.
    pub retained_allowance_bytes: usize,
    /// Whether the prepared table retains rich metadata.
    pub rich_text_preserved: bool,
    /// Number of actual parsed entries, including unsupported extension entries.
    pub entries: u64,
    /// Whether payloads and index were spilled to owned files.
    pub disk_backed: bool,
    /// Retained table/cache allocation estimate, including allocated slots.
    pub managed_bytes: usize,
    /// Combined data and fixed-width index size; OS page cache is additional.
    pub temp_bytes: u64,
    /// Decoded disk-cache hits.
    pub cache_hits: u64,
    /// Decoded disk lookups not satisfied by the cache.
    pub disk_reads: u64,
}

#[derive(Clone)]
enum Entry {
    Text(Box<str>),
    SharedText(Arc<str>),
    Rich(Box<crabxl_core::RichText>),
    Unsupported,
}
impl Entry {
    fn payload_bytes(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::SharedText(s) => s.len().saturating_add(2 * size_of::<usize>()),
            Self::Rich(v) => v.memory_bytes(),
            Self::Unsupported => 0,
        }
    }
    fn view(&self) -> EntryView<'_> {
        match self {
            Self::Text(v) => EntryView::Text(v),
            Self::SharedText(v) => EntryView::Text(v),
            Self::Rich(v) => EntryView::Rich(v),
            Self::Unsupported => EntryView::Unsupported,
        }
    }
    fn value(&self, preserve: bool) -> Result<CellValue> {
        match self {
            Self::Text(s) => Ok(CellValue::text(s.as_ref())),
            Self::SharedText(s) => Ok(CellValue::shared_text(Arc::clone(s))),
            Self::Rich(v) if preserve => {
                let mut value = (**v).clone();
                for run in &mut value.runs {
                    crate::rich_text::unprotect(&mut run.text);
                }
                Ok(CellValue::RichText(Box::new(value)))
            }
            Self::Rich(v) => {
                let text = v.plain_text()?;
                Ok(CellValue::text(if text.contains("x005F_") {
                    text.replace("x005F_", "").into_boxed_str()
                } else {
                    text
                }))
            }
            Self::Unsupported => Err(unsupported()),
        }
    }
    fn plain(value: Box<str>, shared: bool) -> Self {
        if shared {
            Self::SharedText(Arc::from(value))
        } else {
            Self::Text(value)
        }
    }
}
enum EntryView<'a> {
    Text(&'a str),
    Rich(&'a crabxl_core::RichText),
    Unsupported,
}
impl EntryView<'_> {
    fn payload_bytes(&self) -> usize {
        match self {
            Self::Text(v) => v.len(),
            Self::Rich(v) => v.memory_bytes(),
            Self::Unsupported => 0,
        }
    }
}
struct Disk {
    data: BufWriter<File>,
    index: BufWriter<File>,
    bytes: u64,
    payload_bytes: u64,
}
impl Disk {
    fn new(options: &SharedStringOptions) -> Result<Self> {
        let file = || {
            match &options.temp_directory {
                Some(path) => tempfile::tempfile_in(path),
                None => tempfile::tempfile(),
            }
            .map_err(|e| io_error("Cannot create shared-string temporary file", e))
        };
        Ok(Self {
            data: BufWriter::new(file()?),
            index: BufWriter::new(file()?),
            bytes: 0,
            payload_bytes: 0,
        })
    }
    fn push(&mut self, entry: EntryView<'_>, maximum: u64) -> Result<()> {
        let length = if let EntryView::Rich(value) = entry {
            let mut count = CountBytes(0);
            crate::rich_text::write_stored(&mut count, value)
                .map_err(|e| io_error("Cannot measure stored rich text", e))?;
            if count.0 >= RICH_FLAG {
                return Err(limit("Stored rich text is too large"));
            }
            count.0
        } else {
            entry.payload_bytes() as u64
        };
        let next = self
            .bytes
            .checked_add(16)
            .and_then(|n| n.checked_add(length))
            .filter(|n| *n <= maximum)
            .ok_or_else(|| limit("Shared-string temporary byte limit exceeded"))?;
        self.index
            .write_all(&self.payload_bytes.to_le_bytes())
            .map_err(|e| io_error("Cannot write shared-string index", e))?;
        let encoded_length = if matches!(entry, EntryView::Unsupported) {
            u64::MAX
        } else if matches!(entry, EntryView::Rich(_)) {
            length | RICH_FLAG
        } else {
            length
        };
        self.index
            .write_all(&encoded_length.to_le_bytes())
            .map_err(|e| io_error("Cannot write shared-string index", e))?;
        if let EntryView::Text(text) = entry {
            self.data
                .write_all(text.as_bytes())
                .map_err(|e| io_error("Cannot write shared-string payload", e))?;
        }
        if let EntryView::Rich(value) = entry {
            crate::rich_text::write_stored(&mut self.data, value)
                .map_err(|e| io_error("Cannot write rich shared-string payload", e))?;
        }
        self.payload_bytes += length;
        self.bytes = next;
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        self.data
            .flush()
            .map_err(|e| io_error("Cannot flush shared-string payload", e))?;
        self.index
            .flush()
            .map_err(|e| io_error("Cannot flush shared-string index", e))?;
        Ok(())
    }
}
struct CacheEntry {
    id: u64,
    value: Entry,
}
pub(crate) struct SharedStrings {
    memory: Vec<Entry>,
    plain_memory: Vec<Option<Box<str>>>,
    shared_memory: Vec<Option<Arc<str>>>,
    share_values: bool,
    disk: Option<Disk>,
    cache: Vec<Option<CacheEntry>>,
    cache_payload: usize,
    cache_maximum: usize,
    cache_policy_maximum: usize,
    stats: SharedStringStats,
    limits: ResourceLimits,
}
impl SharedStrings {
    pub(crate) fn parse<B: BufRead>(
        input: B,
        part: String,
        limits: ResourceLimits,
        options: &SharedStringOptions,
        preserve_rich: bool,
        share_values: bool,
    ) -> Result<Self> {
        let result = Self::parse_impl(
            input,
            part.clone(),
            limits,
            options,
            preserve_rich,
            share_values,
        );
        result.map_err(|e| e.with_part(part))
    }
    fn parse_impl<B: BufRead>(
        input: B,
        part: String,
        limits: ResourceLimits,
        options: &SharedStringOptions,
        preserve_rich: bool,
        share_values: bool,
    ) -> Result<Self> {
        let allowance_details = memory_allowance(options.memory_policy, limits)?;
        let allowance = allowance_details.retained_data_bytes;
        let mut table = Self {
            limits,
            memory: Vec::new(),
            plain_memory: Vec::new(),
            shared_memory: Vec::new(),
            share_values,
            disk: None,
            cache: Vec::new(),
            cache_payload: 0,
            cache_maximum: options.cache_bytes.min(allowance),
            cache_policy_maximum: options.cache_bytes.min(allowance),
            stats: SharedStringStats {
                rich_text_preserved: preserve_rich,
                budget_bytes: allowance_details.budget_bytes,
                retained_allowance_bytes: allowance,
                ..SharedStringStats::default()
            },
        };
        if options.storage == SharedStringStorage::Disk {
            table.disk = Some(Disk::new(options)?);
        }
        let mut xml = XmlStream::new(input, part, limits.max_part_bytes, limits);
        let mut root = false;
        loop {
            if let Some(entry) = xml.buffered_plain_shared_text(|text| {
                if table.stats.entries >= options.max_entries {
                    return Err(limit("Shared-string entry limit exceeded"));
                }
                crate::encode::validate_xml_text(text)?;
                let entry = if text.contains("x005F_") {
                    Entry::plain(text.replace("x005F_", "").into_boxed_str(), share_values)
                } else if share_values {
                    // Build canonical shared ownership once, without an
                    // intermediate owned text allocation and second copy.
                    Entry::SharedText(Arc::from(text))
                } else {
                    let mut owned = String::new();
                    owned.try_reserve_exact(text.len()).map_err(|cause| {
                        Error::caused_by(
                            ErrorKind::LimitExceeded,
                            "Cannot allocate cell value buffer",
                            cause,
                        )
                    })?;
                    owned.push_str(text);
                    Entry::Text(owned.into_boxed_str())
                };
                Ok(Some(entry))
            })? {
                table.push(entry, options, allowance)?;
                continue;
            }
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e) if frame.depth == 1 => {
                    if frame.scope != Scope::Spreadsheet
                        || e.local_name().as_ref().as_bytes() != b"sst"
                    {
                        return Err(invalid("Invalid shared-string root"));
                    }
                    root = true;
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Spreadsheet
                        && e.local_name().as_ref().as_bytes() == b"si" =>
                {
                    if table.stats.entries >= options.max_entries {
                        return Err(limit("Shared-string entry limit exceeded"));
                    }
                    let entry =
                        parse_entry(&mut xml, limits.max_cell_bytes, preserve_rich, share_values)?;
                    table.push(entry, options, allowance)?;
                }
                Event::Start(_) => return Err(invalid("Unexpected shared-string table element")),
                Event::Text(t) if !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {
                    return Err(invalid("Unexpected text in shared-string table"));
                }
                Event::CData(_) | Event::GeneralRef(_) => {
                    return Err(invalid("Unexpected content in shared-string table"));
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if !root {
            return Err(invalid("Missing shared-string root"));
        }
        if let Some(disk) = &mut table.disk {
            disk.finish()?;
            table.stats.disk_backed = true;
            table.stats.temp_bytes = disk.bytes;
            table.initialize_cache()?;
        }
        Ok(table)
    }
    fn push(
        &mut self,
        entry: Entry,
        options: &SharedStringOptions,
        allowance: usize,
    ) -> Result<()> {
        if self.disk.is_none() {
            let rich = self.stats.rich_text_preserved;
            let (len, capacity, slot) = if rich {
                (
                    self.memory.len(),
                    self.memory.capacity(),
                    size_of::<Entry>(),
                )
            } else if self.share_values {
                (
                    self.shared_memory.len(),
                    self.shared_memory.capacity(),
                    size_of::<Option<Arc<str>>>(),
                )
            } else {
                (
                    self.plain_memory.len(),
                    self.plain_memory.capacity(),
                    size_of::<Option<Box<str>>>(),
                )
            };
            let payload = self.stats.managed_bytes.saturating_sub(capacity * slot);
            let wanted = if len == capacity {
                capacity.saturating_mul(2).max(16)
            } else {
                capacity
            };
            let required = wanted
                .saturating_mul(slot)
                .saturating_add(payload)
                .saturating_add(entry.payload_bytes());
            if required > allowance {
                if options.storage == SharedStringStorage::Memory {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Shared-string table exceeds memory allowance",
                    ));
                }
                let mut disk = Disk::new(options)?;
                if rich {
                    for old in &self.memory {
                        disk.push(old.view(), options.max_temp_bytes)?;
                    }
                } else if self.share_values {
                    for old in &self.shared_memory {
                        disk.push(
                            old.as_deref()
                                .map_or(EntryView::Unsupported, EntryView::Text),
                            options.max_temp_bytes,
                        )?;
                    }
                } else {
                    for old in &self.plain_memory {
                        disk.push(
                            old.as_deref()
                                .map_or(EntryView::Unsupported, EntryView::Text),
                            options.max_temp_bytes,
                        )?;
                    }
                }
                self.memory = Vec::new();
                self.plain_memory = Vec::new();
                self.shared_memory = Vec::new();
                self.disk = Some(disk);
                self.stats.managed_bytes = 0;
            } else {
                let result = if rich {
                    self.memory.try_reserve_exact(wanted - len)
                } else if self.share_values {
                    self.shared_memory.try_reserve_exact(wanted - len)
                } else {
                    self.plain_memory.try_reserve_exact(wanted - len)
                };
                result.map_err(|e| {
                    Error::caused_by(
                        ErrorKind::MemoryBudgetExceeded,
                        "Cannot allocate shared-string table",
                        e,
                    )
                })?;
                let actual_capacity = if rich {
                    self.memory.capacity()
                } else if self.share_values {
                    self.shared_memory.capacity()
                } else {
                    self.plain_memory.capacity()
                };
                let actual = actual_capacity
                    .saturating_mul(slot)
                    .saturating_add(payload)
                    .saturating_add(entry.payload_bytes());
                if actual > allowance {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Shared-string allocation exceeds allowance",
                    ));
                }
                self.stats.managed_bytes = actual;
            }
        }
        if let Some(disk) = &mut self.disk {
            disk.push(entry.view(), options.max_temp_bytes)?;
        } else if self.stats.rich_text_preserved {
            self.memory.push(entry);
        } else if self.share_values {
            self.shared_memory.push(match entry {
                Entry::SharedText(value) => Some(value),
                Entry::Unsupported => None,
                _ => return Err(invalid("Shared plain table received incompatible data")),
            });
        } else {
            self.plain_memory.push(match entry {
                Entry::Text(v) => Some(v),
                Entry::Unsupported => None,
                Entry::Rich(_) | Entry::SharedText(_) => {
                    return Err(invalid("Plain table received unprojected rich data"));
                }
            });
        }
        self.stats.entries += 1;
        Ok(())
    }
    pub(crate) fn get(&mut self, id: u64, preserve: bool) -> Result<CellValue> {
        if id >= self.stats.entries {
            return Err(invalid("Shared-string ID is outside the actual table"));
        }
        if self.disk.is_none() {
            return if self.stats.rich_text_preserved {
                self.memory[id as usize].value(preserve)
            } else if self.share_values {
                self.shared_memory[id as usize].as_ref().map_or_else(
                    || Err(unsupported()),
                    |v| Ok(CellValue::shared_text(Arc::clone(v))),
                )
            } else {
                self.plain_memory[id as usize]
                    .as_ref()
                    .map_or_else(|| Err(unsupported()), |v| Ok(CellValue::text(v.as_ref())))
            };
        }
        let slot = if self.cache.is_empty() {
            None
        } else {
            Some((id % self.cache.len() as u64) as usize)
        };
        if let Some(cached) = slot
            .and_then(|i| self.cache[i].as_ref())
            .filter(|e| e.id == id)
        {
            self.stats.cache_hits += 1;
            return cached.value.value(preserve);
        }
        self.stats.disk_reads += 1;
        let disk = self
            .disk
            .as_mut()
            .ok_or_else(|| invalid("Shared-string disk store is missing"))?;
        disk.index
            .get_mut()
            .seek(SeekFrom::Start(id * 16))
            .map_err(|e| io_error("Cannot seek shared-string index", e))?;
        let mut record = [0u8; 16];
        disk.index
            .get_mut()
            .read_exact(&mut record)
            .map_err(|e| io_error("Cannot read shared-string index", e))?;
        let mut first = [0u8; 8];
        first.copy_from_slice(&record[..8]);
        let mut last = [0u8; 8];
        last.copy_from_slice(&record[8..]);
        let offset = u64::from_le_bytes(first);
        let length = u64::from_le_bytes(last);
        if length == u64::MAX {
            return Err(unsupported());
        }
        let rich = length & RICH_FLAG != 0;
        let length = length & !RICH_FLAG;
        disk.data
            .get_mut()
            .seek(SeekFrom::Start(offset))
            .map_err(|e| io_error("Cannot seek shared-string payload", e))?;
        let entry = if rich {
            let input = std::io::BufReader::with_capacity(
                self.limits.input_buffer_bytes,
                disk.data.get_mut().take(length),
            );
            let mut xml =
                XmlStream::new(input, "<shared-string-store>".into(), length, self.limits);
            xml.next()?;
            xml.next()?;
            let value = crate::rich_text::read_container(
                &mut xml,
                2,
                b"is",
                self.limits.max_cell_bytes,
                true,
            )?;
            while !matches!(xml.next()?.event, Event::Eof) {}
            match value {
                crate::rich_text::ParsedText::Rich(v) => Entry::Rich(v),
                crate::rich_text::ParsedText::Plain(v) => Entry::plain(v, self.share_values),
            }
        } else {
            let length = usize::try_from(length)
                .map_err(|_| limit("Shared-string length exceeds platform bounds"))?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(length).map_err(|e| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot allocate shared-string value",
                    e,
                )
            })?;
            bytes.resize(length, 0);
            disk.data
                .get_mut()
                .read_exact(&mut bytes)
                .map_err(|e| io_error("Cannot read shared-string payload", e))?;
            let text = String::from_utf8(bytes).map_err(|e| {
                Error::caused_by(
                    ErrorKind::InvalidData,
                    "Shared-string store contains invalid UTF-8",
                    e,
                )
            })?;
            Entry::plain(text.into_boxed_str(), self.share_values)
        };
        let value = entry.value(preserve)?;
        let payload_bytes = entry.payload_bytes();
        if let Some(i) = slot {
            let slots_bytes = self.cache.capacity() * size_of::<Option<CacheEntry>>();
            if payload_bytes <= self.cache_maximum.saturating_sub(slots_bytes) {
                if let Some(old) = self.cache[i].take() {
                    self.cache_payload -= old.value.payload_bytes();
                }
                let mut candidate = (i + 1) % self.cache.len();
                while self.cache_payload + payload_bytes > self.cache_maximum - slots_bytes {
                    if let Some(old) = self.cache[candidate].take() {
                        self.cache_payload -= old.value.payload_bytes();
                    }
                    candidate = (candidate + 1) % self.cache.len();
                }
                self.cache_payload += payload_bytes;
                self.cache[i] = Some(CacheEntry { id, value: entry });
                self.stats.managed_bytes = slots_bytes + self.cache_payload;
            }
        }
        Ok(value)
    }
    fn initialize_cache(&mut self) -> Result<()> {
        let slots = (self.cache_maximum / (size_of::<Option<CacheEntry>>() + 128)).min(4096);
        self.cache.try_reserve_exact(slots).map_err(|error| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate shared-string cache",
                error,
            )
        })?;
        if self
            .cache
            .capacity()
            .saturating_mul(size_of::<Option<CacheEntry>>())
            > self.cache_maximum
        {
            return Err(limit("Shared-string cache allocation exceeds budget"));
        }
        self.cache.resize_with(slots, || None);
        self.stats.managed_bytes = self
            .cache
            .capacity()
            .saturating_mul(size_of::<Option<CacheEntry>>());
        Ok(())
    }
    /// Spill a previously prepared Auto table when another managed component needs space.
    /// The existing table remains usable if temporary output fails.
    pub(crate) fn limit_or_spill(
        &mut self,
        options: &SharedStringOptions,
        maximum: usize,
    ) -> Result<()> {
        if self.disk.is_some()
            || self.stats.managed_bytes <= maximum
            || options.storage == SharedStringStorage::Memory
        {
            return self.limit_managed_bytes(maximum);
        }
        let mut disk = Disk::new(options)?;
        if self.stats.rich_text_preserved {
            for entry in &self.memory {
                disk.push(entry.view(), options.max_temp_bytes)?;
            }
        } else if self.share_values {
            for entry in &self.shared_memory {
                disk.push(
                    entry
                        .as_deref()
                        .map_or(EntryView::Unsupported, EntryView::Text),
                    options.max_temp_bytes,
                )?;
            }
        } else {
            for entry in &self.plain_memory {
                disk.push(
                    entry
                        .as_deref()
                        .map_or(EntryView::Unsupported, EntryView::Text),
                    options.max_temp_bytes,
                )?;
            }
        }
        disk.finish()?;
        self.stats.disk_backed = true;
        self.stats.temp_bytes = disk.bytes;
        self.disk = Some(disk);
        self.memory = Vec::new();
        self.plain_memory = Vec::new();
        self.shared_memory = Vec::new();
        self.cache = Vec::new();
        self.cache_payload = 0;
        self.cache_policy_maximum = options.cache_bytes.min(maximum);
        self.cache_maximum = self.cache_policy_maximum;
        self.stats.managed_bytes = 0;
        self.initialize_cache()
    }
    pub(crate) fn minimum_managed_bytes(&self) -> usize {
        if self.disk.is_some() {
            0
        } else {
            self.stats.managed_bytes
        }
    }
    /// Shrink optional decoded cache before another jointly managed component grows.
    pub(crate) fn limit_managed_bytes(&mut self, maximum: usize) -> Result<()> {
        if self.disk.is_none() {
            if self.stats.managed_bytes > maximum {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Shared-string table exceeds aggregate allowance",
                ));
            }
            return Ok(());
        }
        self.cache_maximum = maximum.min(self.cache_policy_maximum);
        let slots = self
            .cache
            .capacity()
            .saturating_mul(size_of::<Option<CacheEntry>>());
        if slots > self.cache_maximum {
            self.cache = Vec::new();
            self.cache_payload = 0;
        } else if slots.saturating_add(self.cache_payload) > self.cache_maximum {
            for entry in &mut self.cache {
                if slots.saturating_add(self.cache_payload) <= self.cache_maximum {
                    break;
                }
                if let Some(old) = entry.take() {
                    self.cache_payload -= old.value.payload_bytes();
                }
            }
        }
        self.stats.managed_bytes = self
            .cache
            .capacity()
            .saturating_mul(size_of::<Option<CacheEntry>>())
            .saturating_add(self.cache_payload);
        if self.cache.is_empty() && self.cache_maximum >= size_of::<Option<CacheEntry>>() + 128 {
            self.initialize_cache()?;
        }
        Ok(())
    }
    pub(crate) fn stats(&self) -> SharedStringStats {
        self.stats
    }
    pub(crate) fn shares_values(&self) -> bool {
        self.share_values
    }
}

fn parse_entry<B: BufRead>(
    xml: &mut XmlStream<B>,
    maximum: usize,
    preserve_rich: bool,
    share_values: bool,
) -> Result<Entry> {
    match crate::rich_text::read_container(xml, 2, b"si", maximum, preserve_rich) {
        Ok(crate::rich_text::ParsedText::Plain(text)) => Ok(Entry::plain(
            if text.contains("x005F_") {
                text.replace("x005F_", "").into_boxed_str()
            } else {
                text
            },
            share_values,
        )),
        Ok(crate::rich_text::ParsedText::Rich(v)) => Ok(Entry::Rich(v)),
        Err(error) if error.kind() == ErrorKind::Unsupported => {
            // Retain the ID of unsupported extension entries while validating XML.
            loop {
                let f = xml.next()?;
                match f.event {
                    Event::End(e)
                        if f.scope == Scope::Spreadsheet
                            && f.depth == 1
                            && e.local_name().as_ref().as_bytes() == b"si" =>
                    {
                        break;
                    }
                    Event::Eof => {
                        return Err(invalid("Incomplete unsupported shared-string entry"));
                    }
                    _ => {}
                }
            }
            Ok(Entry::Unsupported)
        }
        Err(error) => Err(error),
    }
}
const RICH_FLAG: u64 = 1 << 63;
struct CountBytes(u64);
impl Write for CountBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("Rich-text byte count overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn limit(message: &str) -> Error {
    Error::new(ErrorKind::LimitExceeded, message)
}
fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "Selected shared-string extension is not supported yet",
    )
}
fn io_error(message: &str, cause: std::io::Error) -> Error {
    Error::caused_by(ErrorKind::Io, message, cause)
}
