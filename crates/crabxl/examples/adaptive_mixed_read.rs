//! Equivalent value checks for shared strings, shared formulas and styled dates.
use crabxl::{AccessPattern, CellValue, MemoryPolicy, ReadData, Row, WorkbookReader};
fn verify(row: &Row) -> Result<(), Box<dyn std::error::Error>> {
    let index = row.index.get() + 1;
    if row.cells.len() != 3 {
        return Err("Wrong mixed row width".into());
    }
    let CellValue::Text(text) = &row.cells[0].value else {
        return Err("Missing mixed text".into());
    };
    if text.as_str()
        != format!(
            "text-{index:06}-{}",
            "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
        )
    {
        return Err("Incorrect mixed text".into());
    }
    let CellValue::Formula(formula) = &row.cells[1].value else {
        return Err("Missing mixed formula".into());
    };
    if formula.expression() != format!("B{index}+1")
        || formula.cached() != Some(&CellValue::Integer(i64::from(index)))
    {
        return Err("Incorrect mixed expression/cache".into());
    }
    let CellValue::DateTime(date) = &row.cells[2].value else {
        return Err("Missing styled date".into());
    };
    if date.serial() != 43831.0
        || date.epoch() != crabxl::DateEpoch::Windows1900
        || date.kind() != crabxl::DateKind::DateTime
    {
        return Err("Incorrect styled date".into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: adaptive_mixed_read <file> <scan|repeated> <budget_mb> [inspect]")?;
    let access = match args.next().as_deref() {
        Some("scan") => AccessPattern::Scan,
        Some("repeated") => AccessPattern::RepeatedAccess,
        _ => return Err("Invalid access mode".into()),
    };
    let budget: usize = args
        .next()
        .ok_or("Missing budget")?
        .parse::<usize>()?
        .checked_mul(1024 * 1024)
        .ok_or("Budget overflow")?;
    let inspect = args.next().as_deref() == Some("inspect");
    let mut book = WorkbookReader::open(path)?;
    // Include already prepared metadata in the policy accounting.
    book.theme()?;
    let (count, decision, peak) = {
        let mut output = book.read_with_policy("Sheet", access, MemoryPolicy::Budget(budget))?;
        let mut count = 0u64;
        let mut peak = 0usize;
        let retained = output.decision.budget_bytes - output.decision.working_reserve_bytes;
        match &mut output.data {
            ReadData::Streaming(rows) => {
                while let Some(row) = rows.next_row()? {
                    verify(&row)?;
                    count += 3;
                    if inspect {
                        peak = peak.max(rows.managed_retained_bytes());
                        if peak > retained {
                            return Err("Aggregate streaming allowance exceeded".into());
                        }
                    }
                }
            }
            ReadData::Materialized(sheet) => {
                for row in &sheet.rows {
                    verify(row)?;
                    count += 3;
                }
                if inspect {
                    peak = sheet.memory_bytes();
                }
            }
        }
        (count, output.decision.clone(), peak)
    };
    let stats = book
        .shared_string_stats()
        .ok_or("Missing shared-string statistics")?;
    let mut managed = peak;
    if inspect && decision.mode == crabxl::ReadMode::Materialized {
        managed = peak + book.catalog_memory_bytes() + stats.managed_bytes;
        if managed > decision.budget_bytes - decision.working_reserve_bytes {
            return Err("Aggregate model allowance exceeded".into());
        }
    }
    eprintln!(
        "POLICY_STATS {{\"mode\":\"{:?}\",\"budget\":{},\"working\":{},\"initial_data_allowance\":{},\"disk_backed\":{},\"string_managed_bytes\":{},\"temp_bytes\":{},\"inspected_managed_bytes\":{managed}}}",
        decision.mode,
        decision.budget_bytes,
        decision.working_reserve_bytes,
        decision.retained_data_bytes,
        stats.disk_backed,
        stats.managed_bytes,
        stats.temp_bytes
    );
    println!("{count}");
    Ok(())
}
