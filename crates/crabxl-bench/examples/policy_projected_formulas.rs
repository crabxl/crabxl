//! Verify a bounded requested prefix without retaining later shared groups.
use crabxl::{
    AccessPattern, CellValue, MemoryPolicy, ReadData, ReadOptions, RowIndex, WorkbookReader,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: policy_projected_formulas <file> <rows>")?;
    let limit: u32 = args.next().ok_or("Missing row limit")?.parse()?;
    let mut book = WorkbookReader::open(path)?;
    let options = ReadOptions {
        rows: Some(
            RowIndex::new(0)?
                ..=RowIndex::new(limit.checked_sub(1).ok_or("Require positive row limit")?)?,
        ),
        formula_metadata: true,
        ..Default::default()
    };
    let mut output = book.read_with_policy_options(
        "Sheet",
        options,
        AccessPattern::Scan,
        MemoryPolicy::Budget(2 * 1024 * 1024),
    )?;
    let ReadData::Streaming(ref mut rows) = output.data else {
        return Err("Expected projected stream".into());
    };
    let mut count = 0u64;
    while let Some(row) = rows.next_row()? {
        let index = row.index.get() + 1;
        if row.cells.len() != 1 {
            return Err("Incorrect projected row width".into());
        }
        let CellValue::Formula(formula) = &row.cells[0].value else {
            return Err("Missing projected formula".into());
        };
        if formula.expression() != format!("A{index}+1")
            || formula.cached() != Some(&CellValue::Integer(i64::from(index)))
        {
            return Err("Incorrect projected formula/cache".into());
        }
        count += 1;
    }
    if count != u64::from(limit) {
        return Err("Incorrect projected count".into());
    }
    eprintln!("PROJECTED_STATS {}", rows.shared_formula_stats().templates);
    println!("{count}");
    Ok(())
}
