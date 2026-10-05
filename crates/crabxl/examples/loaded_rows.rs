//! Verify lazy canonical models and compare their materialization with standalone models.
use crabxl::{
    CellValue, EditLimits, LoadOptions, LoadedWorkbook, MemoryPolicy, Row, RowIndex,
    WorkbookLimits, WorkbookReader, Worksheet,
};
use std::{fs::File, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: loaded_rows <input.xlsx> [bank|standalone]")?;
    let mode = args.next().unwrap_or_else(|| "bank".into());
    let begin = Instant::now();
    let mut count = 0usize;
    let mut checksum = 0i64;
    let mut sum = |sheet: &Worksheet| -> Result<(), Box<dyn std::error::Error>> {
        for cell in sheet.cells() {
            match cell.value {
                CellValue::Integer(value) => {
                    checksum = checksum.checked_add(value).ok_or("Checksum overflow")?;
                }
                _ => return Err("Example requires integer input".into()),
            }
            count += 1;
        }
        Ok(())
    };
    let (managed, temp) = if mode == "bank" {
        let mut workbook = LoadedWorkbook::with_options(
            File::open(path)?,
            LoadOptions {
                memory_policy: MemoryPolicy::Budget(1024 * 1024 * 1024),
                workbook: WorkbookLimits {
                    max_bytes: 1024 * 1024 * 1024,
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let ids = workbook
            .model()
            .sheets()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for id in ids {
            sum(workbook.sheet(id)?)?;
        }
        let managed = workbook.managed_retained_bytes();
        let temp = workbook
            .shared_string_stats()
            .map_or(0, |stats| stats.temp_bytes);
        (managed, temp)
    } else if mode == "standalone" {
        let mut workbook = WorkbookReader::open(path)?;
        let names = workbook
            .sheets()
            .iter()
            .map(|sheet| sheet.name().to_owned())
            .collect::<Vec<_>>();
        let mut models = Vec::new();
        for name in names {
            let mut sheet = Worksheet::new(&*name, EditLimits::default())?;
            let mut rows = workbook.rows(&name)?;
            let mut row = Row::new(RowIndex::new(0)?);
            while rows.read_row_into(&mut row)? {
                for cell in row.cells.drain(..) {
                    sheet.set(cell)?;
                }
            }
            sheet.mark_clean();
            sum(&sheet)?;
            models.push(sheet);
        }
        let managed = workbook.catalog_memory_bytes()
            + models.iter().map(Worksheet::charged_bytes).sum::<usize>()
            + workbook
                .shared_string_stats()
                .map_or(0, |stats| stats.managed_bytes);
        let temp = workbook
            .shared_string_stats()
            .map_or(0, |stats| stats.temp_bytes);
        (managed, temp)
    } else {
        return Err("Mode must be bank or standalone".into());
    };
    println!(
        "{{\"mode\":\"{mode}\",\"cells\":{count},\"checksum\":{checksum},\"managed_bytes\":{managed},\"sst_temp_bytes\":{temp},\"seconds\":{}}}",
        begin.elapsed().as_secs_f64()
    );
    Ok(())
}
