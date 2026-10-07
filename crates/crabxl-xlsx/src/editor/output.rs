//! Bounded output and editor error checks.
use super::*;

pub(super) fn check_printer_identity(
    original: &crabxl_core::PageSetup,
    replacement: &crabxl_core::PageSetup,
) -> Result<()> {
    if original.printer_relationship != replacement.printer_relationship {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "Changing printer identities requires a package feature graph",
        ));
    }
    Ok(())
}
pub(super) fn contains_date(value: &CellValue) -> bool {
    match value {
        CellValue::DateTime(_) => true,
        CellValue::Formula(formula) => formula.cached().is_some_and(contains_date),
        _ => false,
    }
}
pub(super) fn limit(message: &str) -> Error {
    Error::new(ErrorKind::LimitExceeded, message)
}
pub(super) fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
pub(super) struct PartOutput<W> {
    pub(super) inner: W,
    pub(super) bytes: u64,
    pub(super) maximum: u64,
}
impl<W: Write> Write for PartOutput<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.len() as u64 > self.maximum.saturating_sub(self.bytes) {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Rewritten XML part byte limit exceeded",
            ));
        }
        let count = self.inner.write(data)?;
        self.bytes += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
pub(super) fn emit<W: Write>(writer: &mut Writer<PartOutput<W>>, event: Event<'_>) -> Result<()> {
    writer.write_event(event).map_err(|error| {
        Error::caused_by(
            if error.kind() == io::ErrorKind::FileTooLarge {
                ErrorKind::LimitExceeded
            } else {
                ErrorKind::Io
            },
            "Cannot write preserved XML event",
            error,
        )
    })
}
