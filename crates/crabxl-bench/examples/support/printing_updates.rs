//! Shared benchmark orchestration; all metadata logic remains in the core/editor.
use crabxl::{Error, ErrorKind, Result, SaveOptions, WorkbookEditor};
use std::{fs::File, io::BufWriter};
pub fn run(mut update: impl FnMut(&mut WorkbookEditor<File>, usize) -> Result<()>) -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let source = arguments
        .next()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing source"))?;
    let target = arguments
        .next()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing target"))?;
    let count = arguments
        .next()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing update count"))?
        .parse::<usize>()
        .map_err(|error| Error::caused_by(ErrorKind::InvalidData, "Invalid update count", error))?;
    if count == 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Update count must be positive",
        ));
    }
    let mut editor = WorkbookEditor::open(source)?;
    for index in 0..count {
        update(&mut editor, index)?;
    }
    println!("PRINT_BYTES={}", editor.patch_bytes());
    let output = File::create(target)
        .map_err(|error| Error::caused_by(ErrorKind::Io, "Cannot create output", error))?;
    editor.save(BufWriter::new(output), SaveOptions::default())?;
    println!("{count}");
    Ok(())
}
