//! Parse blocks of plain rows on worker threads.
//!
//! The reading thread inflates the worksheet and cuts it into blocks that end
//! at `</row>`. Workers parse the blocks; results return in order. A worker
//! accepts only rows of plain cells, the same shapes as the buffered cell path.
//! Anything else gives back the block and all later input to the event reader,
//! so results and errors stay those of sequential reading.

use super::{
    BufferedValue, CellHeader, ScalarKind, boolean_value, numeric_value, shared_string_id,
    values::date_value,
};
use crate::strings::StringTable;
use crabxl_core::{CellAddress, DateEpoch, DateKind, ReadOptions, RowIndex, StyleId};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, channel, sync_channel},
    },
    thread::{self, JoinHandle},
};

/// Target block size. Blocks end at the last `</row>` after this many bytes.
const BLOCK: usize = 1 << 20;
/// A block without `</row>` after this many bytes goes to the event reader.
const MAX_BLOCK: usize = 64 << 20;
const MAX_WORKERS: usize = 8;
/// Smaller parts are read on one thread.
pub(crate) const MIN_PART_BYTES: u64 = 4 << 20;

/// A cell whose value is final, or a shared-string ID that the reading
/// thread resolves when the workers have no string table.
pub(super) struct ParsedCell {
    pub(super) address: CellAddress,
    pub(super) style: StyleId,
    pub(super) value: BufferedValue,
}
pub(crate) struct ParsedRow {
    pub(crate) index: RowIndex,
    pub(super) cells: Vec<ParsedCell>,
    /// The row tag without brackets, kept when the reader captures row
    /// formatting and the row has formatting attributes.
    pub(super) tag: Option<Box<str>>,
    /// The first cell whose shared string a worker resolved.
    pub(super) first_text: Option<CellAddress>,
    /// Input bytes of the row, including whitespace before it.
    pub(crate) bytes: usize,
}

struct Context {
    options: ReadOptions,
    capture_dimensions: bool,
    max_event_bytes: usize,
    max_cell_bytes: usize,
    /// The date kind of each cell format, or None without a style catalog.
    date_kinds: Option<Vec<Option<DateKind>>>,
    epoch: DateEpoch,
    /// Taken away when the table moves to disk during the read.
    strings: Mutex<Option<StringTable>>,
    /// Check that rows are plain without decoding their cells.
    validate_only: bool,
}
impl Context {
    /// The date kind of a style, as `StyleRead::kind` gives it, or None when
    /// that would be an error.
    fn date_kind(&self, style: StyleId) -> Option<Option<DateKind>> {
        match &self.date_kinds {
            Some(kinds) if kinds.is_empty() && style.get() == 0 => Some(None),
            Some(kinds) => kinds.get(style.get() as usize).copied(),
            None => (style.get() == 0).then_some(None),
        }
    }
}

/// Worker threads to start, or None when only one CPU is available.
fn worker_count() -> Option<usize> {
    let workers = thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .saturating_sub(1)
        .min(MAX_WORKERS);
    (workers > 0).then_some(workers)
}

/// Parse a block of whole rows. None means the block needs the event reader.
fn parse_block(mut bytes: &[u8], context: &Context) -> Option<Vec<ParsedRow>> {
    let strings = context.strings.lock().ok()?.clone();
    let mut rows = Vec::new();
    loop {
        let start = bytes.len();
        bytes = bytes.trim_ascii_start();
        if bytes.is_empty() {
            return Some(rows);
        }
        let after = bytes.strip_prefix(b"<row")?;
        let end = after.iter().position(|byte| *byte == b'>')?;
        let (attributes, empty) = match after[..end].strip_suffix(b"/") {
            Some(attributes) => (attributes, true),
            None => (&after[..end], false),
        };
        let (index, formatted) = row_index(attributes)?;
        // "row" and its attributes, as the dimension reader expects.
        let tag = if context.capture_dimensions && formatted {
            Some(
                std::str::from_utf8(&bytes[1..4 + attributes.len()])
                    .ok()?
                    .into(),
            )
        } else {
            None
        };
        bytes = &after[end + 1..];
        let mut cells = Vec::new();
        let mut first_text = None;
        if !empty {
            let mut next_column = 0;
            loop {
                bytes = bytes.trim_ascii_start();
                if let Some(rest) = bytes.strip_prefix(b"</row>") {
                    bytes = rest;
                    break;
                }
                let (header, value, consumed) = crate::xml::scalar_value_token(
                    bytes,
                    context.max_event_bytes,
                    context.max_cell_bytes,
                )?;
                bytes = &bytes[consumed..];
                let header = CellHeader::buffered(header.as_bytes(), index, next_column)?;
                let address = header.address;
                if address.row != index || address.column.get() < next_column {
                    return None;
                }
                next_column = address.column.get() + 1;
                if context.validate_only || !context.options.includes(address) {
                    continue;
                }
                let kind = context.date_kind(header.style)?;
                let value = value.trim_ascii();
                let date = |value| {
                    date_value(
                        value,
                        kind,
                        context.epoch,
                        context.options.date_policy,
                        |_| crabxl_core::Error::new(crabxl_core::ErrorKind::InvalidData, ""),
                    )
                    .ok()
                };
                let value = match header.kind {
                    ScalarKind::Numeric => BufferedValue::Direct(date(numeric_value(value).ok()?)?),
                    ScalarKind::Boolean => BufferedValue::Direct(date(boolean_value(value).ok()?)?),
                    ScalarKind::SharedText => {
                        let id = shared_string_id(value).ok()?;
                        match &strings {
                            Some(table) => {
                                first_text.get_or_insert(address);
                                BufferedValue::Direct(date(table.get(id)?)?)
                            }
                            None => BufferedValue::SharedText(id),
                        }
                    }
                    _ => return None,
                };
                cells.push(ParsedCell {
                    address,
                    style: header.style,
                    value,
                });
            }
        }
        rows.push(ParsedRow {
            index,
            cells,
            tag,
            first_text,
            bytes: start - bytes.len(),
        });
    }
}

/// The index of a row tag and whether it has attributes besides `r` and
/// `spans`. Rows without `r`, duplicate or malformed attributes, namespace
/// declarations and values that XML would expand or normalize return None.
fn row_index(mut rest: &[u8]) -> Option<(RowIndex, bool)> {
    let mut index = None;
    let mut extra = false;
    let mut names: Vec<&[u8]> = Vec::new();
    loop {
        let spaced = rest.first().is_some_and(u8::is_ascii_whitespace);
        rest = rest.trim_ascii_start();
        if rest.is_empty() {
            return Some((RowIndex::new(index?).ok()?, extra));
        }
        let equals = rest.iter().position(|byte| *byte == b'=')?;
        let name = &rest[..equals];
        let [quote @ (b'"' | b'\''), tail @ ..] = &rest[equals + 1..] else {
            return None;
        };
        let length = tail.iter().position(|byte| byte == quote)?;
        let value = &tail[..length];
        if !spaced
            || name.is_empty()
            || !name.iter().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.')
            })
            || name.starts_with(b"xmlns")
            || names.contains(&name)
            || value
                .iter()
                .any(|byte| matches!(byte, b'&' | b'<' | b'\t' | b'\n' | b'\r'))
        {
            return None;
        }
        names.push(name);
        match name {
            b"r" => {
                index = Some(
                    std::str::from_utf8(value)
                        .ok()?
                        .parse::<u32>()
                        .ok()?
                        .checked_sub(1)?,
                );
            }
            b"spans" => {}
            _ => extra = true,
        }
        rest = &tail[length + 1..];
    }
}

/// The next block in document order.
pub(crate) enum Block {
    /// The block's bytes and its plain rows.
    Parsed(Arc<Vec<u8>>, Vec<ParsedRow>),
    /// A block the workers did not accept.
    Rejected(Arc<Vec<u8>>),
    /// All blocks were returned. These bytes follow the last one.
    End(Vec<u8>),
}

/// What the reading thread gets next from the pipeline.
pub(super) enum Next {
    Row(ParsedRow),
    /// All rows were read. These bytes follow the last row.
    End(Vec<u8>),
    /// A block needs the event reader. These bytes start at that block.
    Fallback(Vec<u8>),
}

type Job = (u64, Arc<Vec<u8>>);
type Done = (u64, Option<Vec<ParsedRow>>);

pub(crate) struct Pipeline {
    context: Arc<Context>,
    jobs: Option<SyncSender<Job>>,
    results: Receiver<Done>,
    workers: Vec<JoinHandle<()>>,
    /// Blocks sent and not yet consumed, by sequence number.
    blocks: BTreeMap<u64, Arc<Vec<u8>>>,
    finished: BTreeMap<u64, Option<Vec<ParsedRow>>>,
    ready: VecDeque<ParsedRow>,
    /// Input read but not yet sent: the start of an unfinished block.
    carry: Vec<u8>,
    sent: u64,
    consumed: u64,
    input_done: bool,
    in_flight: u64,
}

impl Pipeline {
    /// Start workers, or None when only one CPU is available.
    pub(super) fn start(
        options: &ReadOptions,
        capture_dimensions: bool,
        limits: &crabxl_core::ResourceLimits,
        date_kinds: Option<Vec<Option<DateKind>>>,
        epoch: DateEpoch,
        strings: Option<StringTable>,
    ) -> Option<Self> {
        let workers = worker_count()?;
        Some(Self::spawn(
            workers,
            Context {
                options: options.clone(),
                capture_dimensions,
                max_event_bytes: limits.max_xml_event_bytes,
                max_cell_bytes: limits.max_cell_bytes,
                date_kinds,
                epoch,
                strings: Mutex::new(strings),
                validate_only: false,
            },
        ))
    }

    /// Start workers that only check that rows are plain, or None when only
    /// one CPU is available.
    pub(crate) fn validator(limits: &crabxl_core::ResourceLimits) -> Option<Self> {
        Some(Self::spawn(
            worker_count()?,
            Context {
                options: ReadOptions::default(),
                capture_dimensions: false,
                max_event_bytes: limits.max_xml_event_bytes,
                max_cell_bytes: limits.max_cell_bytes,
                date_kinds: None,
                epoch: DateEpoch::Windows1900,
                strings: Mutex::new(None),
                validate_only: true,
            },
        ))
    }

    fn spawn(workers: usize, context: Context) -> Self {
        let context = Arc::new(context);
        let (jobs, queue) = sync_channel::<Job>(workers);
        let queue = Arc::new(Mutex::new(queue));
        let (done, results) = channel::<Done>();
        let workers = (0..workers)
            .map(|_| {
                let queue = Arc::clone(&queue);
                let done = done.clone();
                let context = Arc::clone(&context);
                thread::spawn(move || {
                    loop {
                        let job = match queue.lock() {
                            Ok(queue) => queue.recv(),
                            Err(_) => return,
                        };
                        let Ok((index, block)) = job else { return };
                        if done.send((index, parse_block(&block, &context))).is_err() {
                            return;
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        let in_flight = workers.len() as u64 * 2;
        Self {
            context,
            jobs: Some(jobs),
            results,
            workers,
            blocks: BTreeMap::new(),
            finished: BTreeMap::new(),
            ready: VecDeque::new(),
            carry: Vec::new(),
            sent: 0,
            consumed: 0,
            input_done: false,
            in_flight,
        }
    }

    /// The next row in document order. `read` appends raw input and returns
    /// false at its end.
    pub(super) fn next(
        &mut self,
        mut read: impl FnMut(&mut Vec<u8>) -> crabxl_core::Result<bool>,
    ) -> crabxl_core::Result<Next> {
        loop {
            if let Some(row) = self.ready.pop_front() {
                return Ok(Next::Row(row));
            }
            match self.next_block(&mut read)? {
                Block::Parsed(_, rows) => self.ready.extend(rows),
                Block::Rejected(block) => {
                    let mut bytes = block.to_vec();
                    bytes.extend(self.take_rest());
                    return Ok(Next::Fallback(bytes));
                }
                Block::End(rest) => return Ok(Next::End(rest)),
            }
        }
    }

    /// The next block in document order. `read` appends raw input and
    /// returns false at its end.
    pub(crate) fn next_block(
        &mut self,
        mut read: impl FnMut(&mut Vec<u8>) -> crabxl_core::Result<bool>,
    ) -> crabxl_core::Result<Block> {
        while !self.input_done && self.sent - self.consumed < self.in_flight {
            self.fill(&mut read)?;
        }
        if self.consumed == self.sent {
            return Ok(Block::End(std::mem::take(&mut self.carry)));
        }
        let rows = loop {
            if let Some(rows) = self.finished.remove(&self.consumed) {
                break rows;
            }
            let Ok((index, rows)) = self.results.recv() else {
                // A worker stopped: the caller reads this block itself.
                break None;
            };
            self.finished.insert(index, rows);
        };
        let block = self.blocks.remove(&self.consumed).unwrap_or_default();
        self.consumed += 1;
        Ok(match rows {
            Some(rows) => Block::Parsed(block, rows),
            None => Block::Rejected(block),
        })
    }

    /// Read input until one more block is sent or the input ends.
    fn fill(
        &mut self,
        read: &mut impl FnMut(&mut Vec<u8>) -> crabxl_core::Result<bool>,
    ) -> crabxl_core::Result<()> {
        loop {
            let searched = self.carry.len().saturating_sub(b"</sheetData".len());
            let more = read(&mut self.carry)?;
            // Rows end at the end of sheetData; the rest goes to the event reader.
            if let Some(found) = memchr::memmem::find(&self.carry[searched..], b"</sheetData") {
                let tail = self.carry.split_off(searched + found);
                self.send_rows();
                self.input_done = true;
                self.carry.extend(tail);
                return Ok(());
            }
            if !more {
                self.input_done = true;
                return Ok(());
            }
            if self.carry.len() >= BLOCK && self.send_rows() {
                return Ok(());
            }
            if self.carry.len() >= MAX_BLOCK {
                // A huge row: the event reader handles it and everything after.
                self.input_done = true;
                return Ok(());
            }
        }
    }

    /// Send the carried input up to its last `</row>`. False if it has none.
    fn send_rows(&mut self) -> bool {
        let Some(end) = memchr::memmem::rfind(&self.carry, b"</row>") else {
            return false;
        };
        let rest = self.carry.split_off(end + b"</row>".len());
        let block = Arc::new(std::mem::replace(&mut self.carry, rest));
        self.blocks.insert(self.sent, Arc::clone(&block));
        let sent = self
            .jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send((self.sent, block)).is_ok());
        if !sent {
            // A worker stopped; the result wait reports it as a fallback.
            self.jobs = None;
        }
        self.sent += 1;
        true
    }

    /// Stop resolving shared strings on the workers. Later blocks return IDs.
    pub(super) fn withdraw_strings(&self) {
        if let Ok(mut strings) = self.context.strings.lock() {
            *strings = None;
        }
    }

    /// Input read ahead whose rows were not returned yet.
    pub(super) fn unread_bytes(&self) -> u64 {
        let blocks: usize = self.blocks.values().map(|block| block.len()).sum();
        let rows: usize = self.ready.iter().map(|row| row.bytes).sum();
        (blocks + rows + self.carry.len()) as u64
    }

    /// All input after the blocks returned so far, in order. The pipeline
    /// returns no more blocks.
    pub(crate) fn take_rest(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for block in self.blocks.values() {
            bytes.extend_from_slice(block);
        }
        bytes.extend_from_slice(&self.carry);
        self.blocks.clear();
        self.carry.clear();
        bytes
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.jobs = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn context(capture_dimensions: bool) -> Context {
        Context {
            options: ReadOptions::default(),
            capture_dimensions,
            max_event_bytes: 1 << 16,
            max_cell_bytes: 1 << 16,
            date_kinds: Some(vec![None, None]),
            epoch: DateEpoch::Windows1900,
            strings: Mutex::new(None),
            validate_only: false,
        }
    }
    fn rows(xml: &str, capture: bool) -> Option<Vec<(u32, usize)>> {
        Some(
            parse_block(xml.as_bytes(), &context(capture))?
                .into_iter()
                .map(|row| (row.index.get(), row.cells.len()))
                .collect(),
        )
    }

    #[test]
    fn plain_rows_parse_and_anything_else_falls_back() {
        let plain = r#"<row r="1" spans="1:2" x14ac:dyDescent="0.3"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>1.5</v></c></row>
            <row r="3"/><row r='4'><c r="C4" s="1"><v>2</v></c></row>"#;
        assert_eq!(rows(plain, false), Some(vec![(0, 2), (2, 0), (3, 1)]));
        assert_eq!(rows("", false), Some(vec![]));
        // Captured row formatting keeps the row tag for the reading thread.
        let block = parse_block(plain.as_bytes(), &context(true)).unwrap();
        assert_eq!(
            block[0].tag.as_deref(),
            Some(r#"row r="1" spans="1:2" x14ac:dyDescent="0.3""#)
        );
        assert!(block[1].tag.is_none() && block[2].tag.is_none());
        for xml in [
            r#"<row><c r="A1"><v>1</v></c></row>"#,
            r#"<row r="1"><c r="A1"><f>1+1</f><v>2</v></c></row>"#,
            r#"<row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row>"#,
            r#"<row r="1"><c r="A1" s="2"/></row>"#,
            r#"<row r="1"><!-- note --></row>"#,
            r#"<row r="1" xmlns="urn:other"></row>"#,
            r#"<row r="0"></row>"#,
            r#"<row r="2"><c r="A1"><v>1</v></c></row>"#,
            r#"<row r="1"><c r="B1"><v>1</v></c><c r="A1"><v>1</v></c></row>"#,
            r#"<row r="1"><c r="A1"><v>1</v></c>"#,
            r#"<x:row r="1"></x:row>"#,
            r#"<row r="1" r="2"></row>"#,
            r#"<row r="1" ht="2" ht="3"></row>"#,
        ] {
            assert_eq!(rows(xml, false), None, "{xml}");
        }
    }
}
