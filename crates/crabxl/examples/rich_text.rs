//! Verify complete rich runs across explicit shared-string placement policies.
use crabxl::{
    CellValue, Color, ColorKind, MemoryPolicy, ReadOptions, Row, RowIndex, SharedStringOptions,
    SharedStringStorage, Underline, WorkbookReader,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "Usage: rich_text <xlsx> <memory|disk|auto> <unique-count> <temp-directory>".into(),
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
        ..Default::default()
    });
    let mut count = 0u64;
    let mut bytes = 0u64;
    {
        let mut rows = book.rows_with_options(
            "Sheet",
            ReadOptions {
                rich_text: true,
                ..Default::default()
            },
        )?;
        let mut row = Row::new(RowIndex::new(0)?);
        let suffix = "x".repeat(96);
        while rows.read_row_into(&mut row)? {
            for cell in &row.cells {
                let CellValue::RichText(value) = &cell.value else {
                    return Err("Expected rich text".into());
                };
                if value.runs.len() != 2
                    || !value.phonetic_runs.is_empty()
                    || value.phonetic_properties.is_some()
                {
                    return Err("Unexpected run/phonetic structure".into());
                }
                let a = &value.runs[0];
                let b = &value.runs[1];
                let f = a.font.as_ref().ok_or("Missing first font")?;
                let g = b.font.as_ref().ok_or("Missing second font")?;
                if a.text.as_ref() != format!("item-{:08}-", count % unique)
                    || b.text.as_ref() != suffix
                    || f.bold != Some(true)
                    || f.italic != Some(false)
                    || f.color
                        != Some(Color {
                            kind: ColorKind::Theme(3),
                            tint: Some(0.25),
                        })
                    || g.name.as_deref() != Some("Aptos")
                    || g.underline != Some(Underline::Double)
                {
                    return Err("Rich-text value/font/order mismatch".into());
                }
                bytes += (a.text.len() + b.text.len()) as u64;
                count += 1;
            }
        }
    }
    let s = book.shared_string_stats().ok_or("Missing SST stats")?;
    println!("{count} {bytes}");
    eprintln!(
        "STRINGS {{\"entries\":{},\"disk_backed\":{},\"managed_bytes\":{},\"temp_bytes\":{},\"cache_hits\":{},\"disk_reads\":{}}}",
        s.entries, s.disk_backed, s.managed_bytes, s.temp_bytes, s.cache_hits, s.disk_reads
    );
    Ok(())
}
