//! Explicit caller-owned theme catalog inspection without loading worksheet cells.
use crabxl::WorkbookReader;
use std::{fs::File, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: theme_read <input.xlsx>")?;
    let start = Instant::now();
    let mut reader = WorkbookReader::new(File::open(path)?)?;
    let catalog = reader
        .read_theme_catalog()?
        .ok_or("Workbook has no theme")?;
    println!(
        "fonts={} catalog_bytes={} opaque_bytes={} elapsed_seconds={:.6}",
        catalog.major_fonts.supplemental.len() + catalog.minor_fonts.supplemental.len(),
        catalog.memory_bytes(),
        reader.theme_memory_bytes(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
