//! Measure disk-backed point metadata without materializing a worksheet model.
use crabxl::{
    Cell, CellAddress, CellValue, Hyperlink, Row, RowIndex, StyleId, WorkbookWriter, WriteOptions,
};
use std::{error::Error, fs::File, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let count: u32 = args.next().ok_or("Expected row count")?.parse()?;
    let path = args.next().ok_or("Expected output path")?;
    let mut writer = WorkbookWriter::new(WriteOptions {
        max_metadata_bytes: 128 * 1024,
        ..Default::default()
    })?;
    writer.start_sheet("Links")?;
    let mut row = Row::new(RowIndex::new(0)?);
    row.cells.push(Cell {
        address: CellAddress::new(0, 0)?,
        value: CellValue::text("value"),
        style: StyleId::new(0),
    });
    let start = Instant::now();
    let mut checksum = 0_u64;
    for index in 0..count {
        row.index = RowIndex::new(index)?;
        let address = CellAddress::new(index, 0)?;
        row.cells[0].address = address;
        let target = format!("https://example.org/item/{index}?x=1&y=2#part");
        checksum += target.len() as u64;
        writer.write_row_with_hyperlinks(&row, &[(address, Hyperlink::external(target))])?;
    }
    let append = start.elapsed().as_secs_f64();
    let start = Instant::now();
    writer.close_sheet()?;
    let close = start.elapsed().as_secs_f64();
    let stats = writer.stats();
    if stats.rows != u64::from(count) || stats.cells != u64::from(count) {
        return Err("Output cardinality mismatch".into());
    }
    let retained_temp = writer.temporary_bytes();
    let start = Instant::now();
    writer.finish(File::create(path)?)?;
    let finish = start.elapsed().as_secs_f64();
    #[cfg(target_os = "linux")]
    let rss: u64 = std::fs::read_to_string("/proc/self/status")?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse().ok())
        })
        .ok_or("Missing kernel RSS high-water")?;
    #[cfg(not(target_os = "linux"))]
    let rss = 0_u64;
    println!(
        "rows={count} append={append:.6} close={close:.6} finish={finish:.6} checksum={checksum} peak_temp_bytes={} retained_temp_before_packaging={retained_temp} peak_rss_kib={rss}",
        stats.peak_temp_bytes
    );
    Ok(())
}
