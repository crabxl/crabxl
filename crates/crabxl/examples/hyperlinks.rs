//! Generate and scan sparse external links using the canonical native model.
use crabxl::{
    CellAddress, EditLimits, Hyperlink, WorkbookReader, WorkbookWriter, Worksheet, WriteOptions,
};
use std::{error::Error, fs::File, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let count: u32 = args.next().ok_or("Expected row count")?.parse()?;
    let path = args.next().ok_or("Expected output path")?;
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
    let mut reader = WorkbookReader::open(&path)?;
    let links = reader.hyperlinks("Links")?;
    if links.len() != count as usize {
        return Err("Hyperlink count mismatch".into());
    }
    let checksum: u64 = links
        .iter()
        .map(|(address, link)| {
            u64::from(address.row.get())
                + link.target.as_ref().map_or(0, |value| value.len()) as u64
        })
        .sum();
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
        "rows={count} prepare={prepare:.6} save={save:.6} scan={:.6} checksum={checksum} peak_temp_xml_bytes={temporary} peak_rss_kib={peak_rss_kib}",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
