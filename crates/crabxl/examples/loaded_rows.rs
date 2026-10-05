//! Verify lazy canonical models and compare their materialization with standalone models.
use crabxl::{
    CellAddress, CellValue, EditLimits, LoadOptions, LoadedWorkbook, MemoryPolicy, Row, RowIndex,
    WorkbookLimits, WorkbookReader, Worksheet,
};
use std::{fs::File, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: loaded_rows <input.xlsx> [bank|standalone|bank-edit] [output.xlsx]")?;
    let mode = args.next().unwrap_or_else(|| "bank".into());
    let output = args.next();
    let mut output_bytes = 0u64;
    let mut verified_edit = false;
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
    let (managed, temp) = if mode == "bank" || mode == "bank-edit" {
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
        for id in ids.iter().copied() {
            sum(workbook.sheet(id)?)?;
        }
        if mode == "bank-edit" {
            let target = output.as_ref().ok_or("Edit mode requires output path")?;
            let id = *ids.first().ok_or("No source sheets")?;
            let address = CellAddress::new(0, 0)?;
            let old = match workbook.sheet(id)?.get(address).ok_or("Missing A1")?.value {
                CellValue::Integer(value) => value,
                _ => return Err("Expected integer A1".into()),
            };
            workbook.set_value(id, address, CellValue::Integer(42))?;
            workbook.save_path(target, crabxl::SaveOptions::default())?;
            output_bytes = std::fs::metadata(target)?.len();
            let mut saved = WorkbookReader::open(target)?;
            let names = saved
                .sheets()
                .iter()
                .map(|sheet| sheet.name().to_owned())
                .collect::<Vec<_>>();
            let mut saved_count = 0usize;
            let mut saved_sum = 0i64;
            for name in names {
                let mut rows = saved.rows(&name)?;
                while let Some(row) = rows.next_row()? {
                    for cell in row.cells {
                        let CellValue::Integer(value) = cell.value else {
                            return Err("Unexpected saved value".into());
                        };
                        saved_sum = saved_sum
                            .checked_add(value)
                            .ok_or("Saved checksum overflow")?;
                        saved_count += 1;
                    }
                }
            }
            if saved_count != count || saved_sum != checksum - old + 42 {
                return Err("Edited output mismatch".into());
            }
            verified_edit = true;
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
        return Err("Mode must be bank, standalone or bank-edit".into());
    };
    println!(
        "{{\"mode\":\"{mode}\",\"cells\":{count},\"checksum\":{checksum},\"managed_bytes\":{managed},\"sst_temp_bytes\":{temp},\"output_bytes\":{output_bytes},\"verified_edit\":{verified_edit},\"seconds\":{}}}",
        begin.elapsed().as_secs_f64()
    );
    Ok(())
}
