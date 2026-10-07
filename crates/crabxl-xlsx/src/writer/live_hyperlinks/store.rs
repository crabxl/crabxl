//! Disk-backed mutable group payloads and fixed-width owner events.
use super::*;
use std::io::{Read, SeekFrom};

const SLOT_BYTES: u64 = 16;
pub(in crate::writer) struct Store {
    pub(in crate::writer) index: NamedTempFile,
    pub(in crate::writer) values: NamedTempFile,
    pub(in crate::writer) count: u64,
    pub(in crate::writer) bytes: u64,
}
impl Store {
    pub(in crate::writer) fn heap_bytes(&self) -> usize {
        self.index.path().as_os_str().len() + self.values.path().as_os_str().len()
    }
    pub(in crate::writer) fn validate(&self, group: u64) -> Result<()> {
        if group >= self.count {
            return Err(state("Unknown writer-local hyperlink group"));
        }
        Ok(())
    }
    pub(in crate::writer) fn update(
        &mut self,
        group: u64,
        payload: &[u8],
        new: bool,
    ) -> Result<()> {
        if new {
            if group != self.count {
                return Err(state("Invalid new hyperlink group identity"));
            }
        } else {
            self.validate(group)?;
        }
        let offset = self.values.seek(SeekFrom::End(0)).map_err(store_io)?;
        self.values.write_all(payload).map_err(store_io)?;
        self.index
            .seek(SeekFrom::Start(
                group
                    .checked_mul(SLOT_BYTES)
                    .ok_or_else(|| limit("Hyperlink group index overflows"))?,
            ))
            .map_err(store_io)?;
        self.index
            .write_all(&offset.to_le_bytes())
            .map_err(store_io)?;
        self.index
            .write_all(&(payload.len() as u64).to_le_bytes())
            .map_err(store_io)?;
        self.bytes += payload.len() as u64 + if new { SLOT_BYTES } else { 0 };
        if new {
            self.count += 1;
        }
        Ok(())
    }
    pub(in crate::writer) fn read(&mut self, group: u64, maximum: usize) -> Result<Hyperlink> {
        self.validate(group)?;
        self.index
            .seek(SeekFrom::Start(group * SLOT_BYTES))
            .map_err(store_io)?;
        let offset = read_u64(&mut self.index)?;
        let length = read_u64(&mut self.index)?;
        let length =
            usize::try_from(length).map_err(|_| limit("Hyperlink payload length overflows"))?;
        // Decoding retains the input and the owned fields together.
        if length
            .saturating_mul(2)
            .saturating_add(size_of::<Hyperlink>())
            > maximum
        {
            return Err(limit("Hyperlink group decode exceeds metadata allowance"));
        }
        let mut payload = Vec::new();
        payload.try_reserve_exact(length).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot reserve hyperlink group payload",
                cause,
            )
        })?;
        payload.resize(length, 0);
        self.values
            .seek(SeekFrom::Start(offset))
            .map_err(store_io)?;
        self.values.read_exact(&mut payload).map_err(store_io)?;
        let mut input = payload.as_slice();
        let mut fields = [None, None, None, None, None, None];
        for field in &mut fields {
            let length = read_u64(&mut input)?;
            if length == u64::MAX {
                continue;
            }
            let length =
                usize::try_from(length).map_err(|_| state("Invalid hyperlink field length"))?;
            let text = input
                .get(..length)
                .ok_or_else(|| state("Truncated hyperlink group payload"))?;
            *field = Some(
                std::str::from_utf8(text)
                    .map_err(|cause| {
                        Error::caused_by(
                            ErrorKind::InvalidData,
                            "Invalid stored hyperlink text",
                            cause,
                        )
                    })?
                    .into(),
            );
            input = &input[length..];
        }
        if input.len() != 1 || input[0] > 1 {
            return Err(state("Invalid hyperlink group flags"));
        }
        let [
            reference,
            target,
            location,
            display,
            tooltip,
            relationship_id,
        ] = fields;
        Ok(Hyperlink {
            reference,
            target,
            location,
            display,
            tooltip,
            relationship_id,
            external: input[0] == 1,
        })
    }
    pub(in crate::writer) fn into_files(self) -> [NamedTempFile; 2] {
        [self.index, self.values]
    }
}

pub(in crate::writer) fn encode(link: &Hyperlink, maximum: usize) -> Result<Vec<u8>> {
    let mut output = RowBuffer {
        data: Vec::new(),
        maximum,
    };
    for field in [
        &link.reference,
        &link.target,
        &link.location,
        &link.display,
        &link.tooltip,
        &link.relationship_id,
    ] {
        output
            .write_all(
                &field
                    .as_ref()
                    .map_or(u64::MAX, |text| text.len() as u64)
                    .to_le_bytes(),
            )
            .map_err(store_io)?;
        if let Some(text) = field {
            output.write_all(text.as_bytes()).map_err(store_io)?;
        }
    }
    output
        .write_all(&[u8::from(link.external)])
        .map_err(store_io)?;
    Ok(output.data)
}
fn read_u64(input: &mut impl Read) -> Result<u64> {
    let mut bytes = [0; 8];
    input.read_exact(&mut bytes).map_err(store_io)?;
    Ok(u64::from_le_bytes(bytes))
}
fn store_io(cause: io::Error) -> Error {
    io_error("Cannot access disk-backed hyperlink metadata", cause)
}

pub(in crate::writer) struct Events {
    pub(in crate::writer) output: BufWriter<NamedTempFile>,
    pub(in crate::writer) count: u64,
}
impl Events {
    pub(in crate::writer) fn heap_bytes(&self) -> usize {
        self.output.capacity() + self.output.get_ref().path().as_os_str().len()
    }
    pub(in crate::writer) fn into_file(self) -> NamedTempFile {
        self.output.into_parts().0
    }
    pub(in crate::writer) fn append(&mut self, address: CellAddress, group: u64) -> Result<()> {
        self.output
            .write_all(&address.row.get().to_le_bytes())
            .map_err(store_io)?;
        self.output
            .write_all(&address.column.get().to_le_bytes())
            .map_err(store_io)?;
        self.output
            .write_all(&group.to_le_bytes())
            .map_err(store_io)?;
        self.count += 1;
        Ok(())
    }
    pub(in crate::writer) fn rewind(&mut self) -> Result<()> {
        self.output.rewind().map_err(store_io)
    }
    pub(in crate::writer) fn next(&mut self) -> Result<(CellAddress, u64)> {
        let input = self.output.get_mut();
        let mut coordinates = [0; 8];
        input.read_exact(&mut coordinates).map_err(store_io)?;
        let row = u32::from_le_bytes([
            coordinates[0],
            coordinates[1],
            coordinates[2],
            coordinates[3],
        ]);
        let col = u32::from_le_bytes([
            coordinates[4],
            coordinates[5],
            coordinates[6],
            coordinates[7],
        ]);
        Ok((CellAddress::new(row, col)?, read_u64(input)?))
    }
}
