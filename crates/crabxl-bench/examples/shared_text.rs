//! Verify every shared-string value while measuring explicit storage policies.
use crabxl::{
    CellValue, MemoryPolicy, Row, RowIndex, SharedStringOptions, SharedStringStorage,
    WorkbookReader,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "Usage: shared_text <xlsx> <memory|disk|auto> <unique-count> <temp-directory>".into(),
        );
    }
    let unique: u64 = args[3].parse()?;
    if unique == 0 {
        return Err("Unique count must be positive".into());
    }
    let storage = match args[2].as_str() {
        "memory" => SharedStringStorage::Memory,
        "disk" => SharedStringStorage::Disk,
        "auto" => SharedStringStorage::Auto,
        _ => return Err("Invalid storage policy".into()),
    };
    let mut book = WorkbookReader::open(&args[1])?;
    book.set_shared_string_options(SharedStringOptions {
        storage,
        memory_policy: MemoryPolicy::Budget(if storage == SharedStringStorage::Memory {
            512 * 1024 * 1024
        } else {
            16 * 1024 * 1024
        }),
        cache_bytes: 1024 * 1024,
        temp_directory: Some(args[4].clone().into()),
        ..SharedStringOptions::default()
    });
    let mut count = 0u64;
    let mut bytes = 0u64;
    {
        let mut rows = book.rows("Sheet")?;
        let mut row = Row::new(RowIndex::new(0)?);
        let suffix = "x".repeat(96);
        while rows.read_row_into(&mut row)? {
            for cell in &row.cells {
                let CellValue::Text(text) = &cell.value else {
                    return Err("Expected text cell".into());
                };
                let expected = format!("item-{:08}-{suffix}", count % unique);
                if text.as_str() != expected {
                    return Err("Shared-string value/order mismatch".into());
                }
                bytes += text.as_str().len() as u64;
                count += 1;
            }
        }
    }
    let stats = book
        .shared_string_stats()
        .ok_or("Shared strings were not prepared")?;
    println!("{count} {bytes}");
    eprintln!(
        "STRINGS {{\"entries\":{},\"disk_backed\":{},\"managed_bytes\":{},\"temp_bytes\":{},\"cache_hits\":{},\"disk_reads\":{}}}",
        stats.entries,
        stats.disk_backed,
        stats.managed_bytes,
        stats.temp_bytes,
        stats.cache_hits,
        stats.disk_reads
    );
    Ok(())
}
