// SPDX-License-Identifier: MIT
// SST event flow adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Storage, budgeting and relationship integration are CrabXL implementations.
// Source provenance: third_party/ports.json.

use crate::{
    memory_allowance,
    xml::{Scope, XmlStream, append_xml_text},
};
use crabxl_core::{CellValue, Error, ErrorKind, MemoryPolicy, ResourceLimits, Result};
use quick_xml::events::Event;
use std::{
    fs::File,
    io::{BufRead, BufWriter, Read, Seek, SeekFrom, Write},
    path::PathBuf,
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
    /// Number of actual parsed entries, including unsupported rich entries.
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

enum Entry {
    Text(Box<str>),
    Unsupported,
}
impl Entry {
    fn payload_bytes(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Unsupported => 0,
        }
    }
    fn value(&self) -> Result<CellValue> {
        match self {
            Self::Text(s) => Ok(CellValue::text(s.as_ref())),
            Self::Unsupported => Err(unsupported()),
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
    fn push(&mut self, entry: &Entry, maximum: u64) -> Result<()> {
        let length = entry.payload_bytes() as u64;
        let next = self
            .bytes
            .checked_add(16)
            .and_then(|n| n.checked_add(length))
            .filter(|n| *n <= maximum)
            .ok_or_else(|| limit("Shared-string temporary byte limit exceeded"))?;
        self.index
            .write_all(&self.payload_bytes.to_le_bytes())
            .map_err(|e| io_error("Cannot write shared-string index", e))?;
        let encoded_length = if matches!(entry, Entry::Unsupported) {
            u64::MAX
        } else {
            length
        };
        self.index
            .write_all(&encoded_length.to_le_bytes())
            .map_err(|e| io_error("Cannot write shared-string index", e))?;
        if let Entry::Text(text) = entry {
            self.data
                .write_all(text.as_bytes())
                .map_err(|e| io_error("Cannot write shared-string payload", e))?;
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
    text: Box<str>,
}
pub(crate) struct SharedStrings {
    memory: Vec<Entry>,
    disk: Option<Disk>,
    cache: Vec<Option<CacheEntry>>,
    cache_payload: usize,
    cache_maximum: usize,
    stats: SharedStringStats,
}
impl SharedStrings {
    pub(crate) fn parse<B: BufRead>(
        input: B,
        part: String,
        limits: ResourceLimits,
        options: &SharedStringOptions,
    ) -> Result<Self> {
        let result = Self::parse_impl(input, part.clone(), limits, options);
        result.map_err(|e| e.with_part(part))
    }
    fn parse_impl<B: BufRead>(
        input: B,
        part: String,
        limits: ResourceLimits,
        options: &SharedStringOptions,
    ) -> Result<Self> {
        let allowance_details = memory_allowance(options.memory_policy, limits)?;
        let allowance = allowance_details.retained_data_bytes;
        let mut table = Self {
            memory: Vec::new(),
            disk: None,
            cache: Vec::new(),
            cache_payload: 0,
            cache_maximum: options.cache_bytes.min(allowance),
            stats: SharedStringStats {
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
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e) if frame.depth == 1 => {
                    if frame.scope != Scope::Spreadsheet || e.local_name().as_ref() != b"sst" {
                        return Err(invalid("Invalid shared-string root"));
                    }
                    root = true;
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Spreadsheet
                        && e.local_name().as_ref() == b"si" =>
                {
                    if table.stats.entries >= options.max_entries {
                        return Err(limit("Shared-string entry limit exceeded"));
                    }
                    let entry = parse_entry(&mut xml, limits.max_cell_bytes)?;
                    table.push(entry, options, allowance)?;
                }
                Event::Start(_) => return Err(invalid("Unexpected shared-string table element")),
                Event::Text(t) if !t.iter().all(u8::is_ascii_whitespace) => {
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
            let slots = (table.cache_maximum / (size_of::<Option<CacheEntry>>() + 128)).min(4096);
            table.cache.try_reserve_exact(slots).map_err(|e| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot allocate shared-string cache",
                    e,
                )
            })?;
            if table.cache.capacity() * size_of::<Option<CacheEntry>>() > table.cache_maximum {
                return Err(limit("Shared-string cache allocation exceeds budget"));
            }
            table.cache.resize_with(slots, || None);
            table.stats.managed_bytes = table.cache.capacity() * size_of::<Option<CacheEntry>>();
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
            let payload = self
                .stats
                .managed_bytes
                .saturating_sub(self.memory.capacity() * size_of::<Entry>());
            let capacity = if self.memory.len() == self.memory.capacity() {
                self.memory.capacity().saturating_mul(2).max(16)
            } else {
                self.memory.capacity()
            };
            let required = capacity
                .saturating_mul(size_of::<Entry>())
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
                for old in &self.memory {
                    disk.push(old, options.max_temp_bytes)?;
                }
                self.memory = Vec::new();
                self.disk = Some(disk);
                self.stats.managed_bytes = 0;
            } else {
                self.memory
                    .try_reserve_exact(capacity - self.memory.len())
                    .map_err(|e| {
                        Error::caused_by(
                            ErrorKind::MemoryBudgetExceeded,
                            "Cannot allocate shared-string table",
                            e,
                        )
                    })?;
                let actual = self
                    .memory
                    .capacity()
                    .saturating_mul(size_of::<Entry>())
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
            disk.push(&entry, options.max_temp_bytes)?;
        } else {
            self.memory.push(entry);
        }
        self.stats.entries += 1;
        Ok(())
    }
    pub(crate) fn get(&mut self, id: u64) -> Result<CellValue> {
        if id >= self.stats.entries {
            return Err(invalid("Shared-string ID is outside the actual table"));
        }
        if self.disk.is_none() {
            return self.memory[id as usize].value();
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
            return Ok(CellValue::text(cached.text.as_ref()));
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
            .seek(SeekFrom::Start(offset))
            .map_err(|e| io_error("Cannot seek shared-string payload", e))?;
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
        let value = CellValue::text(text.as_str());
        if let Some(i) = slot {
            let slots_bytes = self.cache.capacity() * size_of::<Option<CacheEntry>>();
            if text.len() <= self.cache_maximum.saturating_sub(slots_bytes) {
                if let Some(old) = self.cache[i].take() {
                    self.cache_payload -= old.text.len();
                }
                let mut candidate = (i + 1) % self.cache.len();
                while self.cache_payload + text.len() > self.cache_maximum - slots_bytes {
                    if let Some(old) = self.cache[candidate].take() {
                        self.cache_payload -= old.text.len();
                    }
                    candidate = (candidate + 1) % self.cache.len();
                }
                self.cache_payload += text.len();
                self.cache[i] = Some(CacheEntry {
                    id,
                    text: text.into_boxed_str(),
                });
                self.stats.managed_bytes = slots_bytes + self.cache_payload;
            }
        }
        Ok(value)
    }
    pub(crate) fn stats(&self) -> SharedStringStats {
        self.stats
    }
}

fn parse_entry<B: BufRead>(xml: &mut XmlStream<B>, maximum: usize) -> Result<Entry> {
    let mut text = String::new();
    let mut seen = false;
    let mut rich = false;
    let mut in_text = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref() == b"t"
                    && !rich =>
            {
                if seen {
                    return Err(invalid("Shared string has duplicate plain text elements"));
                }
                seen = true;
                in_text = true;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && matches!(e.local_name().as_ref(), b"r" | b"rPh" | b"phoneticPr") =>
            {
                rich = true;
            }
            Event::Start(_) if rich => {}
            Event::Start(_) => return Err(invalid("Unexpected shared-string entry element")),
            event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_))
                if in_text && frame.depth == 3 =>
            {
                append_xml_text(&mut text, &event, maximum)?;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"t" =>
            {
                in_text = false
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref() == b"si" =>
            {
                // Match the pinned public shared-string reader's protected-literal behavior.
                return Ok(if rich {
                    Entry::Unsupported
                } else {
                    Entry::Text(if text.contains("x005F_") {
                        text.replace("x005F_", "").into_boxed_str()
                    } else {
                        text.into_boxed_str()
                    })
                });
            }
            Event::End(_) if rich => {}
            Event::Text(t) if rich || t.iter().all(u8::is_ascii_whitespace) => {}
            Event::CData(_) | Event::GeneralRef(_) if rich => {}
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid("Invalid shared-string entry content")),
        }
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
        "Rich or phonetic shared strings are not supported yet",
    )
}
fn io_error(message: &str, cause: std::io::Error) -> Error {
    Error::caused_by(ErrorKind::Io, message, cause)
}
