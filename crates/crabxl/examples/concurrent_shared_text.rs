//! Verify independent-sheet scans under concurrency-aware Auto allowances.
use crabxl::{
    AccessPattern, AutoMemory, CellValue, MemoryPolicy, ReadData, Row, RowIndex,
    SharedStringOptions, SharedStringStats, WorkbookReader,
};
use std::{path::Path, thread};
type Failure = Box<dyn std::error::Error + Send + Sync>;
struct Summary {
    cells: u64,
    budget: usize,
    strings: SharedStringStats,
}
fn scan(
    path: &Path,
    sheet: &str,
    unique: u64,
    operations: u16,
    available: u64,
    directory: &Path,
) -> Result<Summary, Failure> {
    let mut book = WorkbookReader::open(path)?;
    let policy = MemoryPolicy::Auto(AutoMemory {
        available_bytes: Some(available),
        headroom_bytes: 0,
        concurrent_operations: operations,
        ..Default::default()
    });
    book.set_shared_string_options(SharedStringOptions {
        memory_policy: policy,
        cache_bytes: 1024 * 1024,
        temp_directory: Some(directory.to_owned()),
        ..Default::default()
    });
    let mut count = 0u64;
    let budget;
    {
        let output = book.read_with_policy(sheet, AccessPattern::Scan, policy)?;
        budget = output.decision.budget_bytes;
        let ReadData::Streaming(mut rows) = output.data else {
            return Err("Scan unexpectedly materialized".into());
        };
        let suffix = "x".repeat(96);
        let mut row = Row::new(RowIndex::new(0)?);
        while rows.read_row_into(&mut row)? {
            for cell in &row.cells {
                let CellValue::Text(text) = &cell.value else {
                    return Err("Expected text".into());
                };
                let expected = format!("item-{:08}-{suffix}", count % unique);
                if text.as_str() != expected {
                    return Err("Incorrect shared string/order".into());
                }
                count += 1;
            }
        }
    }
    let strings = book
        .shared_string_stats()
        .ok_or("Missing string statistics")?;
    if strings.budget_bytes > budget {
        return Err("String allowance exceeded operation budget".into());
    }
    Ok(Summary {
        cells: count,
        budget,
        strings,
    })
}
fn main() -> Result<(), Failure> {
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.len() != 6 {
        return Err("Usage: concurrent_shared_text <file> <serial|parallel> <unique-count> <available-MiB> <temp-directory>".into());
    }
    let path = Path::new(&arguments[1]);
    let unique = arguments[3].parse::<u64>()?;
    let available = arguments[4]
        .parse::<u64>()?
        .checked_mul(1024 * 1024)
        .ok_or("Availability overflows")?;
    if unique == 0 {
        return Err("Unique count must be positive".into());
    }
    let directory = Path::new(&arguments[5]);
    let summaries = match arguments[2].as_str() {
        "serial" => vec![
            scan(path, "Sheet", unique, 1, available, directory)?,
            scan(path, "Other", unique, 1, available, directory)?,
        ],
        "parallel" => thread::scope(|scope| -> Result<Vec<Summary>, Failure> {
            let first = scope.spawn(|| scan(path, "Sheet", unique, 2, available, directory));
            let second = scope.spawn(|| scan(path, "Other", unique, 2, available, directory));
            Ok(vec![
                first.join().map_err(|_| "First worker panicked")??,
                second.join().map_err(|_| "Second worker panicked")??,
            ])
        })?,
        _ => return Err("Unknown execution mode".into()),
    };
    for summary in summaries {
        println!(
            "{{\"cells\":{},\"budget_bytes\":{},\"disk_backed\":{},\"managed_bytes\":{},\"temp_bytes\":{},\"cache_hits\":{},\"disk_reads\":{}}}",
            summary.cells,
            summary.budget,
            summary.strings.disk_backed,
            summary.strings.managed_bytes,
            summary.strings.temp_bytes,
            summary.strings.cache_hits,
            summary.strings.disk_reads
        );
    }
    Ok(())
}
