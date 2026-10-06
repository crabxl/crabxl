//! Verify lazy canonical models and compare their materialization with standalone models.
use crabxl::{
    CellAddress, CellValue, EditLimits, LoadOptions, LoadedWorkbook, MemoryPolicy, Row, RowIndex,
    WorkbookLimits, WorkbookReader, Worksheet,
};
use std::{fs::File, time::Instant};
fn model_options() -> LoadOptions {
    LoadOptions {
        memory_policy: MemoryPolicy::Budget(1024 * 1024 * 1024),
        resources: crabxl::ResourceLimits {
            max_materialized_bytes: 1024 * 1024 * 1024,
            ..Default::default()
        },
        workbook: WorkbookLimits {
            max_bytes: 1024 * 1024 * 1024,
            sheet: EditLimits {
                max_bytes: 768 * 1024 * 1024,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or(
        "Usage: loaded_rows <input.xlsx> [bank|standalone|bank-edit|bank-active|bank-visibility|bank-deferred] [output.xlsx]",
    )?;
    let mode = args.next().unwrap_or_else(|| "bank".into());
    let output = args.next();
    let mut output_bytes = 0u64;
    let mut verified_edit = false;
    let materialized_cells;
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
    let (managed, temp) = if matches!(
        mode.as_str(),
        "bank-active"
            | "bank-visibility"
            | "bank-deferred"
            | "bank-rename"
            | "bank-reorder"
            | "bank-create"
            | "bank-copy"
    ) {
        let mut workbook = LoadedWorkbook::with_options(
            File::open(path)?,
            if mode == "bank-copy" {
                model_options()
            } else {
                LoadOptions::default()
            },
        )?;
        let id = workbook
            .model()
            .sheets()
            .nth(1)
            .ok_or("Active mode requires two sheets")?
            .0;
        if mode == "bank-visibility" {
            let first = workbook.model().sheets().next().ok_or("No source sheet")?.0;
            workbook.set_sheet_visibility(first, crabxl::SheetVisibility::Hidden)?;
        } else if mode == "bank-deferred" {
            workbook.set_active_view_index(-1)?;
        } else {
            workbook.set_active_sheet(id)?;
        }
        let renamed = "Renamed<&\" \u{65b0}";
        if matches!(mode.as_str(), "bank-rename" | "bank-reorder") {
            workbook.rename_sheet(id, renamed)?;
        }
        if mode == "bank-reorder" {
            workbook.move_sheet(id, 0)?;
            workbook.set_active_sheet(id)?;
        }
        if mode == "bank-create" {
            let created = workbook.create_sheet("Added")?;
            workbook.upsert_value(created, CellAddress::new(0, 0)?, CellValue::Integer(42))?;
        }
        let expected_materialized = if mode == "bank-copy" {
            let source = workbook.model().sheets().next().ok_or("Missing source")?.0;
            let copied = workbook.copy_sheet(source, "Copied")?;
            workbook.set_value(copied, CellAddress::new(0, 0)?, CellValue::Integer(42))?;
            if workbook.is_materialized(id) {
                return Err("Copy loaded an unrelated sheet".into());
            }
            workbook.model().cell_count()
        } else {
            usize::from(mode == "bank-create")
        };
        let selected = usize::from(mode != "bank-reorder");
        let target = output.as_ref().ok_or("Active mode requires output path")?;
        workbook.save_path(target, crabxl::SaveOptions::default())?;
        output_bytes = std::fs::metadata(target)?.len();
        materialized_cells = workbook.model().cell_count();
        if materialized_cells != expected_materialized {
            return Err("Active selection eagerly materialized cells".into());
        }
        let mut saved = WorkbookReader::open(target)?;
        if saved.active_index() != Some(selected) {
            return Err("Saved active selection mismatch".into());
        }
        if matches!(mode.as_str(), "bank-rename" | "bank-reorder")
            && saved.sheets()[selected].name() != renamed
        {
            return Err("Saved sheet name mismatch".into());
        }
        if mode == "bank-visibility"
            && saved.sheets()[0].visibility() != crabxl::SheetVisibility::Hidden
        {
            return Err("Saved visibility mismatch".into());
        }
        if workbook.model().active_sheet() != Some(id) {
            return Err("Loaded active selection is not synchronized".into());
        }
        if mode == "bank-create" {
            if saved.sheets().len() != 3 || saved.sheets()[2].name() != "Added" {
                return Err("Created catalog mismatch".into());
            }
            let mut rows = saved.rows("Added")?;
            let row = rows.next_row()?.ok_or("Missing created row")?;
            if row.cells.len() != 1 || row.cells[0].value != CellValue::Integer(42) {
                return Err("Created cell mismatch".into());
            }
        }
        if mode == "bank-copy" {
            if saved.sheets().len() != 3 || saved.sheets()[2].name() != "Copied" {
                return Err("Copied catalog mismatch".into());
            }
            let mut rows = saved.rows("Copied")?;
            let row = rows.next_row()?.ok_or("Missing copied row")?;
            if row.cells.first().ok_or("Missing copied A1")?.value != CellValue::Integer(42) {
                return Err("Copied edit mismatch".into());
            }
        }
        let names = saved
            .sheets()
            .iter()
            .map(|sheet| sheet.name().to_owned())
            .collect::<Vec<_>>();
        for name in names {
            let mut rows = saved.rows(&name)?;
            while let Some(row) = rows.next_row()? {
                for cell in row.cells {
                    let CellValue::Integer(value) = cell.value else {
                        return Err("Expected saved integer".into());
                    };
                    count += 1;
                    checksum = checksum
                        .checked_add(value)
                        .ok_or("Saved checksum overflow")?;
                }
            }
        }
        verified_edit = true;
        (
            workbook.managed_retained_bytes(),
            workbook
                .shared_string_stats()
                .map_or(0, |stats| stats.temp_bytes),
        )
    } else if matches!(
        mode.as_str(),
        "bank" | "bank-edit" | "bank-append" | "bank-structure"
    ) {
        let mut workbook = LoadedWorkbook::with_options(File::open(path)?, model_options())?;
        let ids = workbook
            .model()
            .sheets()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for id in ids.iter().copied() {
            sum(workbook.sheet(id)?)?;
        }
        if matches!(
            mode.as_str(),
            "bank-edit" | "bank-append" | "bank-structure"
        ) {
            let target = output.as_ref().ok_or("Edit mode requires output path")?;
            let id = *ids.first().ok_or("No source sheets")?;
            let address = CellAddress::new(0, 0)?;
            let old = match workbook.sheet(id)?.get(address).ok_or("Missing A1")?.value {
                CellValue::Integer(value) => value,
                _ => return Err("Expected integer A1".into()),
            };
            let appended_row = if mode == "bank-structure" {
                workbook.insert_rows(id, RowIndex::new(0)?, 2)?;
                workbook.insert_columns(id, crabxl::ColumnIndex::new(0)?, 1)?;
                None
            } else if mode == "bank-append" {
                let expected = workbook.sheet(id)?.row_extent();
                let row = workbook.append(id, vec![CellValue::Integer(42); 10])?;
                if row.get() != expected {
                    return Err("Append cursor mismatch".into());
                }
                Some(row)
            } else {
                workbook.set_value(id, address, CellValue::Integer(42))?;
                None
            };
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
            let first_name = names.first().ok_or("No saved sheets")?.clone();
            for name in names {
                let mut rows = saved.rows(&name)?;
                while let Some(row) = rows.next_row()? {
                    for cell in row.cells {
                        let CellValue::Integer(value) = cell.value else {
                            return Err("Unexpected saved value".into());
                        };
                        if let Some(appended) = appended_row
                            && name == first_name
                            && cell.address.row == appended
                            && value != 42
                        {
                            return Err("Appended cell mismatch".into());
                        }
                        if mode == "bank-structure"
                            && name == first_name
                            && (i64::from(cell.address.row.get()) != value / 10 + 2
                                || i64::from(cell.address.column.get()) != value % 10 + 1)
                        {
                            return Err("Shifted source coordinate mismatch".into());
                        }
                        saved_sum = saved_sum
                            .checked_add(value)
                            .ok_or("Saved checksum overflow")?;
                        saved_count += 1;
                    }
                }
            }
            let (expected_count, expected_sum) = if appended_row.is_some() {
                (count + 10, checksum + 420)
            } else if mode == "bank-structure" {
                (count, checksum)
            } else {
                (count, checksum - old + 42)
            };
            if saved_count != expected_count || saved_sum != expected_sum {
                return Err("Edited output mismatch".into());
            }
            verified_edit = true;
        }
        let managed = workbook.managed_retained_bytes();
        materialized_cells = workbook.model().cell_count();
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
        materialized_cells = models.iter().map(Worksheet::len).sum();
        let temp = workbook
            .shared_string_stats()
            .map_or(0, |stats| stats.temp_bytes);
        (managed, temp)
    } else {
        return Err(
            "Mode must be bank, standalone, bank-edit, bank-active, bank-visibility or bank-deferred".into(),
        );
    };
    println!(
        "{{\"mode\":\"{mode}\",\"cells\":{count},\"checksum\":{checksum},\"materialized_cells\":{materialized_cells},\"managed_bytes\":{managed},\"sst_temp_bytes\":{temp},\"output_bytes\":{output_bytes},\"verified_edit\":{verified_edit},\"seconds\":{}}}",
        begin.elapsed().as_secs_f64()
    );
    Ok(())
}
