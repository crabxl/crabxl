//! Deflate a large part on several threads, as pigz does, and add it to the
//! ZIP as already-compressed data. Chunks end with a sync flush, so their
//! outputs join into one deflate stream; each chunk is primed with the
//! previous 32 KiB, so the ratio matches single-threaded compression closely.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufReader, BufWriter, Seek, SeekFrom, Write},
    sync::mpsc::{Receiver, Sender, channel},
};
use zip::{ZipArchive, ZipWriter};

const CHUNK: usize = 128 * 1024;
const WINDOW: usize = 32 * 1024;
/// Smaller parts compress on the calling thread through the ZIP writer.
pub(crate) const MIN_BYTES: u64 = 4 * 1024 * 1024;

/// Worker threads for a part of `size` bytes, or None to use the ZIP writer.
pub(crate) fn threads(level: Option<u8>, size: u64) -> Option<usize> {
    if level == Some(0) || size < MIN_BYTES {
        return None;
    }
    let workers = crate::pool::workers();
    (workers > 0).then_some(workers)
}

type Done = (u64, io::Result<Vec<u8>>);

/// Sends a chunk's result once; a task that panics sends an error.
struct Reply {
    index: u64,
    done: Sender<Done>,
    result: Option<io::Result<Vec<u8>>>,
}
impl Reply {
    fn send(mut self, result: io::Result<Vec<u8>>) {
        self.result = Some(result);
    }
}
impl Drop for Reply {
    fn drop(&mut self) {
        let result = self
            .result
            .take()
            .unwrap_or_else(|| Err(io::Error::other("Parallel deflate worker stopped")));
        let _ = self.done.send((self.index, result));
    }
}

/// A writer that deflates on worker threads into a one-entry temporary ZIP.
pub(crate) struct ParallelDeflate {
    output: BufWriter<File>,
    buffer: Vec<u8>,
    dictionary: Vec<u8>,
    crc: crc32fast::Hasher,
    size: u64,
    compressed: u64,
    level: u32,
    done: Sender<Done>,
    results: Receiver<Done>,
    sent: u64,
    written: u64,
    pending: BTreeMap<u64, Vec<u8>>,
    in_flight: u64,
    header: u64,
}
impl ParallelDeflate {
    pub(crate) fn new(
        level: Option<u8>,
        threads: usize,
        directory: Option<&std::path::Path>,
    ) -> io::Result<Self> {
        let file = match directory {
            Some(directory) => tempfile::tempfile_in(directory)?,
            None => tempfile::tempfile()?,
        };
        let mut output = BufWriter::with_capacity(64 * 1024, file);
        // Room for the local header, written once the sizes are known.
        let header = local_header(0, 0, 0).len() as u64;
        output.write_all(&vec![0; header as usize])?;
        let (done, results) = channel::<Done>();
        let level = u32::from(level.unwrap_or(6));
        Ok(Self {
            output,
            buffer: Vec::with_capacity(CHUNK),
            dictionary: Vec::new(),
            crc: crc32fast::Hasher::new(),
            size: 0,
            compressed: 0,
            level,
            done,
            results,
            sent: 0,
            written: 0,
            pending: BTreeMap::new(),
            in_flight: threads as u64 + 1,
            header,
        })
    }
    fn dispatch(&mut self, last: bool) -> io::Result<()> {
        while self.sent - self.written >= self.in_flight {
            self.receive()?;
        }
        let data = std::mem::replace(&mut self.buffer, Vec::with_capacity(CHUNK));
        let next = data[data.len().saturating_sub(WINDOW)..].to_vec();
        let dictionary = std::mem::replace(&mut self.dictionary, next);
        let reply = Reply {
            index: self.sent,
            done: self.done.clone(),
            result: None,
        };
        let level = self.level;
        crate::pool::spawn(move || reply.send(deflate(level, &dictionary, &data, last)));
        self.sent += 1;
        Ok(())
    }
    fn receive(&mut self) -> io::Result<()> {
        let (index, data) = self
            .results
            .recv()
            .map_err(|_| io::Error::other("Parallel deflate worker stopped"))?;
        self.pending.insert(index, data?);
        while let Some(data) = self.pending.remove(&self.written) {
            self.output.write_all(&data)?;
            self.compressed += data.len() as u64;
            self.written += 1;
        }
        Ok(())
    }
    /// Finish the stream and add it to `zip` as the entry `name`.
    pub(crate) fn finish_into<W: Write + Seek>(
        mut self,
        zip: &mut ZipWriter<W>,
        name: &str,
    ) -> io::Result<()> {
        self.dispatch(true)?;
        while self.written < self.sent {
            self.receive()?;
        }
        let crc = self.crc.clone().finalize();
        let central = central_directory(crc, self.compressed, self.size, self.header);
        self.output.write_all(&central)?;
        self.output.flush()?;
        let mut file = self.output.get_ref().try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&local_header(crc, self.compressed, self.size))?;
        file.seek(SeekFrom::Start(0))?;
        let mut archive = ZipArchive::new(BufReader::new(file)).map_err(io::Error::other)?;
        let entry = archive.by_index_raw(0).map_err(io::Error::other)?;
        zip.raw_copy_file_rename(entry, name)
            .map_err(io::Error::other)
    }
}
impl Write for ParallelDeflate {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut taken = 0;
        while taken < data.len() {
            if self.buffer.len() == CHUNK {
                self.dispatch(false)?;
            }
            let count = (CHUNK - self.buffer.len()).min(data.len() - taken);
            self.buffer.extend_from_slice(&data[taken..taken + count]);
            self.crc.update(&data[taken..taken + count]);
            self.size += count as u64;
            taken += count;
        }
        Ok(taken)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Raw-deflate one chunk; non-final chunks end byte-aligned with a sync flush.
fn deflate(level: u32, dictionary: &[u8], data: &[u8], last: bool) -> io::Result<Vec<u8>> {
    use flate2::{Compress, Compression, FlushCompress, Status};
    let mut compress = Compress::new(Compression::new(level), false);
    if !dictionary.is_empty() {
        compress
            .set_dictionary(dictionary)
            .map_err(io::Error::other)?;
    }
    let flush = if last {
        FlushCompress::Finish
    } else {
        FlushCompress::Sync
    };
    let mut output = Vec::with_capacity(data.len() / 2 + 1024);
    loop {
        let consumed = compress.total_in() as usize;
        let status = compress
            .compress_vec(&data[consumed..], &mut output, flush)
            .map_err(io::Error::other)?;
        let done = if last {
            status == Status::StreamEnd
        } else {
            compress.total_in() as usize == data.len() && output.len() < output.capacity()
        };
        if done {
            return Ok(output);
        }
        output.reserve(output.capacity().max(4096));
    }
}
const NAME: &[u8] = b"part";
// DOS date 1980-01-01, the date the ZIP writer gives other entries.
const DATE: u16 = (1 << 5) | 1;
fn zip64(compressed: u64, size: u64) -> bool {
    compressed >= u64::from(u32::MAX) || size >= u64::from(u32::MAX)
}
/// Local header; always reserves the zip64 field so its length is fixed.
fn local_header(crc: u32, compressed: u64, size: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&DATE.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(&(NAME.len() as u16).to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(NAME);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&compressed.to_le_bytes());
    out
}
fn central_directory(crc: u32, compressed: u64, size: u64, header: u64) -> Vec<u8> {
    let large = zip64(compressed, size);
    let mut entry = Vec::with_capacity(96);
    entry.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    entry.extend_from_slice(&45u16.to_le_bytes());
    entry.extend_from_slice(&45u16.to_le_bytes());
    entry.extend_from_slice(&0u16.to_le_bytes());
    entry.extend_from_slice(&8u16.to_le_bytes());
    entry.extend_from_slice(&0u16.to_le_bytes());
    entry.extend_from_slice(&DATE.to_le_bytes());
    entry.extend_from_slice(&crc.to_le_bytes());
    let short = |value: u64| if large { u32::MAX } else { value as u32 };
    entry.extend_from_slice(&short(compressed).to_le_bytes());
    entry.extend_from_slice(&short(size).to_le_bytes());
    entry.extend_from_slice(&(NAME.len() as u16).to_le_bytes());
    entry.extend_from_slice(&(if large { 20u16 } else { 0 }).to_le_bytes());
    // Comment length, disk number, internal and external attributes.
    entry.extend_from_slice(&[0; 10]);
    entry.extend_from_slice(&0u32.to_le_bytes());
    entry.extend_from_slice(NAME);
    if large {
        entry.extend_from_slice(&1u16.to_le_bytes());
        entry.extend_from_slice(&16u16.to_le_bytes());
        entry.extend_from_slice(&size.to_le_bytes());
        entry.extend_from_slice(&compressed.to_le_bytes());
    }
    let offset = header + compressed;
    let mut out = entry.clone();
    let length = entry.len() as u64;
    if offset >= u64::from(u32::MAX) {
        let record = offset + length;
        out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
        out.extend_from_slice(&44u64.to_le_bytes());
        out.extend_from_slice(&45u16.to_le_bytes());
        out.extend_from_slice(&45u16.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&record.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
    }
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(length as u32).to_le_bytes());
    out.extend_from_slice(&(offset.min(u64::from(u32::MAX)) as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::io::{Cursor, Read};

    #[test]
    fn chunks_join_into_one_stream_that_reads_back() {
        // Repetitive and varied text across many chunks, as in worksheet XML.
        let mut data = Vec::new();
        for row in 0..120_000u32 {
            write!(
                data,
                "<row r=\"{row}\"><c><v>{}</v></c></row>",
                row * 7919 % 10007
            )
            .unwrap();
        }
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let mut packed = ParallelDeflate::new(None, 3, None).unwrap();
        packed.write_all(&data).unwrap();
        packed
            .finish_into(&mut zip, "xl/worksheets/sheet1.xml")
            .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut entry = archive.by_name("xl/worksheets/sheet1.xml").unwrap();
        let compressed = entry.compressed_size();
        let mut read = Vec::new();
        entry.read_to_end(&mut read).unwrap();
        assert_eq!(read, data);
        // Within a few percent of one-thread level 6 output.
        let mut single =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
        single.write_all(&data).unwrap();
        let single = single.finish().unwrap().len() as u64;
        assert!(
            compressed <= single + single / 20,
            "{compressed} vs {single}"
        );
    }

    #[test]
    fn empty_and_zip64_directories_are_valid() {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        ParallelDeflate::new(Some(1), 2, None)
            .unwrap()
            .finish_into(&mut zip, "empty.xml")
            .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut read = Vec::new();
        archive
            .by_name("empty.xml")
            .unwrap()
            .read_to_end(&mut read)
            .unwrap();
        assert!(read.is_empty());
        // A directory for a 5 GB part parses with zip64 sizes and offsets.
        let directory = central_directory(1, 5 << 30, 6 << 30, 50);
        assert_eq!(&directory[46..50], NAME);
        assert!(
            directory
                .windows(4)
                .any(|w| w == 0x0606_4b50u32.to_le_bytes())
        );
    }
}
