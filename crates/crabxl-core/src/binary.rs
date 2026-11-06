//! Shared encoded assets. Each reader has its own position; disk sources keep no payload in RAM.
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
enum Storage {
    Memory(Arc<[u8]>),
    File(Arc<Mutex<File>>),
}

/// Immutable encoded image, embedded package or cache-record source.
/// A file passed here must not be modified through another handle.
#[derive(Clone, Debug)]
pub struct BinaryPayload {
    storage: Storage,
    length: u64,
}
impl BinaryPayload {
    /// Adopt an owned file at any initial cursor position. The payload starts at byte zero.
    pub fn from_file(file: File) -> io::Result<Self> {
        let length = file.metadata()?.len();
        Ok(Self {
            storage: Storage::File(Arc::new(Mutex::new(file))),
            length,
        })
    }
    /// Encoded payload size.
    pub const fn len(&self) -> u64 {
        self.length
    }
    /// Whether the payload is empty.
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }
    /// Retained payload allocation; file-backed data does not count as RAM.
    pub fn memory_bytes(&self) -> usize {
        match &self.storage {
            Storage::Memory(bytes) => bytes.len(),
            Storage::File(_) => size_of::<Mutex<File>>(),
        }
    }
    /// An independent reader borrowing this payload.
    pub fn reader(&self) -> BinaryReader<'_> {
        BinaryReader {
            payload: self,
            position: 0,
        }
    }
    /// Stream the encoded payload to an output without materializing it.
    pub fn write_to(&self, output: &mut impl Write) -> io::Result<u64> {
        let written = io::copy(&mut self.reader(), output)?;
        if written != self.length {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Encoded asset was truncated",
            ));
        }
        Ok(written)
    }
}
impl From<Arc<[u8]>> for BinaryPayload {
    fn from(bytes: Arc<[u8]>) -> Self {
        Self {
            length: bytes.len() as u64,
            storage: Storage::Memory(bytes),
        }
    }
}
impl From<Vec<u8>> for BinaryPayload {
    fn from(bytes: Vec<u8>) -> Self {
        Self::from(Arc::<[u8]>::from(bytes))
    }
}
impl From<&[u8]> for BinaryPayload {
    fn from(bytes: &[u8]) -> Self {
        Self::from(Arc::<[u8]>::from(bytes))
    }
}
impl PartialEq for BinaryPayload {
    fn eq(&self, other: &Self) -> bool {
        if self.length != other.length {
            return false;
        }
        match (&self.storage, &other.storage) {
            (Storage::Memory(a), Storage::Memory(b)) => a == b,
            (Storage::File(a), Storage::File(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Independent seek position over a borrowed encoded asset.
pub struct BinaryReader<'a> {
    payload: &'a BinaryPayload,
    position: u64,
}
impl Read for BinaryReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let remaining = self.payload.length.saturating_sub(self.position);
        let count = remaining.min(output.len() as u64) as usize;
        if count == 0 {
            return Ok(0);
        }
        let count = match &self.payload.storage {
            Storage::Memory(bytes) => {
                let start = self.position as usize;
                output[..count].copy_from_slice(&bytes[start..start + count]);
                count
            }
            Storage::File(file) => {
                let mut file = file
                    .lock()
                    .map_err(|_| io::Error::other("Encoded asset lock is poisoned"))?;
                file.seek(SeekFrom::Start(self.position))?;
                file.read(&mut output[..count])?
            }
        };
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Encoded asset was truncated",
            ));
        }
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for BinaryReader<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::End(offset) => i128::from(self.payload.length) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        self.position = u64::try_from(position).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid encoded asset seek")
        })?;
        Ok(self.position)
    }
}
