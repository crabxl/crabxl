//! Generate and scan sparse external links using the canonical native model.
use crabxl::{
    CellAddress, EditLimits, Hyperlink, WorkbookReader, WorkbookWriter, Worksheet, WriteOptions,
};
use std::{error::Error, fs::File, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let count: u32 = args.next().ok_or("Expected row count")?.parse()?;
    let path = args.next().ok_or("Expected output path")?;
    let mode = args.next().unwrap_or_else(|| "scan".into());
    let start = Instant::now();
    let mut sheet = Worksheet::new("Links", EditLimits::default())?;
    for row in 0..count {
        let mut link =
            Hyperlink::external(format!("https://example.org/item/{row}?a=1&b=2#section"));
        link.tooltip = Some(format!("Item {row}").into());
        sheet.set_hyperlink(CellAddress::new(row, 0)?, Some(link))?;
    }
    let prepare = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.write_worksheet(&sheet)?;
    let temporary = writer.stats().peak_temp_bytes;
    writer.finish(File::create(&path)?)?;
    let save = start.elapsed().as_secs_f64();
    drop(sheet);
    let start = Instant::now();
    let checksum = match mode.as_str() {
        "scan" => checksum(&WorkbookReader::open(&path)?.hyperlinks("Links")?, count)?,
        "combined" => {
            let mut reader = WorkbookReader::open(&path)?;
            let mut rows = reader.rows("Links")?;
            rows.capture_hyperlinks();
            let mut row = crabxl::Row::new(crabxl::RowIndex::new(0)?);
            let mut cells = 0;
            while rows.read_row_into(&mut row)? {
                cells += row.cells.len();
            }
            if cells != count as usize {
                return Err("Cell count mismatch".into());
            }
            let declarations = rows.take_hyperlinks();
            drop(rows);
            checksum(
                &reader.resolve_hyperlinks("Links", declarations, 16 * 1024 * 1024)?,
                count,
            )?
        }
        "loaded" => {
            let mut loaded = crabxl::LoadedWorkbook::open(&path)?;
            let id = loaded
                .sheet_id("Links")
                .ok_or("Missing generated worksheet")?;
            let sum = checksum(loaded.hyperlinks(id)?, count)?;
            if loaded.sheet(id)?.len() != count as usize {
                return Err("Materialized cell count mismatch".into());
            }
            sum
        }
        "edit" => {
            if count == 0 {
                return Err("Edit mode requires at least one point".into());
            }
            let mut loaded = crabxl::LoadedWorkbook::open(&path)?;
            let id = loaded
                .sheet_id("Links")
                .ok_or("Missing generated worksheet")?;
            let last = CellAddress::new(count - 1, 0)?;
            let mut changed = loaded
                .hyperlinks(id)?
                .get(last)
                .ok_or("Missing last hyperlink")?
                .clone();
            changed.target = Some("https://example.org/changed?x=1&y=2#new".into());
            loaded.set_hyperlink(id, last, Some(changed))?;
            let sum = checksum(loaded.hyperlinks(id)?, count)?;
            loaded.save(
                File::create(format!("{path}.edited.xlsx"))?,
                Default::default(),
            )?;
            sum
        }
        _ => return Err("Expected scan, combined, loaded or edit mode".into()),
    };
    #[cfg(target_os = "linux")]
    let peak_rss_kib = std::fs::read_to_string("/proc/self/status")?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .ok_or("Missing kernel high-water RSS")?;
    #[cfg(not(target_os = "linux"))]
    let peak_rss_kib = 0_u64;
    println!(
        "mode={mode} rows={count} prepare={prepare:.6} save={save:.6} scan={:.6} checksum={checksum} peak_temp_xml_bytes={temporary} peak_rss_kib={peak_rss_kib}",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn checksum(links: &crabxl::Hyperlinks, count: u32) -> Result<u64, Box<dyn Error>> {
    if links.len() != count as usize {
        return Err("Hyperlink count mismatch".into());
    }
    Ok(links
        .iter()
        .map(|(address, link)| {
            u64::from(address.row.get())
                + link.target.as_ref().map_or(0, |value| value.len()) as u64
        })
        .sum())
}
