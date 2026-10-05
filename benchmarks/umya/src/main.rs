//! Public-API comparison of editable Rust models and numeric cell streams.
use crabxl::{CellAddress, CellValue, LoadOptions, LoadedWorkbook, MemoryPolicy, WorkbookReader};
use std::{fs::File, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let input = Path::new(args.get(1).ok_or("Missing input path")?);
    let mode = args.get(2).ok_or("Missing comparison mode")?;
    let target = args.get(3).map(Path::new);
    let start = Instant::now();
    let mut count = 0usize;
    let mut checksum = 0i64;
    let mut sheets = 0usize;
    let mut output_bytes = 0u64;
    let mut sum = |value: i64| {
        count += 1;
        checksum = checksum
            .checked_add(value)
            .expect("Generated checksum fits i64");
    };
    if mode.starts_with("write-") {
        let rows = args
            .get(4)
            .ok_or("Write mode requires row count")?
            .parse::<u32>()?;
        let output = target.ok_or("Write mode requires output")?;
        if matches!(
            mode.as_str(),
            "write-rust_xlsxwriter-normal" | "write-rust_xlsxwriter-constant"
        ) {
            let mut book = rust_xlsxwriter::Workbook::new();
            let sheet = if mode.ends_with("constant") {
                book.add_worksheet_with_constant_memory()
            } else {
                book.add_worksheet()
            };
            sheet.set_name("Sheet")?;
            for row in 0..rows {
                for column in 0..10u16 {
                    sheet.write_number(row, column, f64::from(row) * 10.0 + f64::from(column))?;
                }
            }
            book.save(output)?;
        } else if mode == "write-crabxl-stream" {
            let mut writer = crabxl::WorkbookWriter::new(crabxl::WriteOptions::default())?;
            writer.start_sheet("Sheet")?;
            let mut cells = crabxl::Row::new(crabxl::RowIndex::new(0)?);
            for row in 0..rows {
                cells.index = crabxl::RowIndex::new(row)?;
                cells.cells.clear();
                for column in 0..10 {
                    cells.cells.push(crabxl::Cell {
                        address: CellAddress::new(row, column)?,
                        value: CellValue::Integer(i64::from(row) * 10 + i64::from(column)),
                        style: crabxl::StyleId::new(0),
                    });
                }
                writer.write_row(&cells)?;
            }
            writer.finish(File::create(output)?)?;
        } else if mode == "write-crabxl-model" {
            let mut book = crabxl::Workbook::new(crabxl::WorkbookLimits {
                max_bytes: 1024 * 1024 * 1024,
                sheet: crabxl::EditLimits {
                    max_bytes: 1024 * 1024 * 1024,
                    ..Default::default()
                },
                ..Default::default()
            })?;
            let id = book.create_sheet("Sheet")?;
            {
                let mut sheet = book.sheet_mut(id)?;
                for row in 0..rows {
                    for column in 0..10 {
                        sheet.set(crabxl::Cell {
                            address: CellAddress::new(row, column)?,
                            value: CellValue::Integer(i64::from(row) * 10 + i64::from(column)),
                            style: crabxl::StyleId::new(0),
                        })?;
                    }
                }
            }
            let mut writer = crabxl::WorkbookWriter::new(crabxl::WriteOptions::default())?;
            writer.write_workbook(&book)?;
            writer.finish(File::create(output)?)?;
        } else if mode == "write-umya-model" {
            let mut book = umya_spreadsheet::new_file();
            book.sheet_mut(0)?.set_name("Sheet");
            let sheet = book.sheet_mut(0)?;
            for row in 0..rows {
                for column in 0..10 {
                    sheet
                        .cell_mut((column + 1, row + 1))
                        .set_value_number(f64::from(row) * 10.0 + f64::from(column));
                }
            }
            umya_spreadsheet::writer::xlsx::write(&book, output)?;
        } else {
            return Err("Unknown write comparison mode".into());
        }
        count = rows as usize * 10;
        checksum = (count as i64) * (count as i64 - 1) / 2;
        sheets = 1;
        output_bytes = std::fs::metadata(output)?.len();
    } else if mode.starts_with("calamine-") {
        use calamine::{Data, DataRef, Reader};
        let mut book: calamine::Xlsx<_> = calamine::open_workbook(input)?;
        let names = book.sheet_names();
        sheets = names.len();
        for name in names {
            if mode == "calamine-stream" {
                let mut cells = book.worksheet_cells_reader(&name)?;
                while let Some(cell) = cells.next_cell()? {
                    let value = match cell.get_value() {
                        DataRef::Int(value) => *value,
                        DataRef::Float(value) if value.fract() == 0.0 => *value as i64,
                        _ => return Err("Expected numeric input".into()),
                    };
                    sum(value);
                }
            } else if mode == "calamine-range" {
                let range = book.worksheet_range(&name)?;
                for cell in range.used_cells() {
                    let value = match cell.2 {
                        Data::Int(value) => *value,
                        Data::Float(value) if value.fract() == 0.0 => *value as i64,
                        _ => return Err("Expected numeric input".into()),
                    };
                    sum(value);
                }
            } else {
                return Err("Unknown calamine mode".into());
            }
        }
    } else if mode.starts_with("umya-") {
        if mode == "umya-stream" {
            // These are the two declarations emitted by workbook_demo.
            for name in ["Copy", "Renamed"] {
                umya_spreadsheet::reader::xlsx::read_sheet_by_name_stream(input, name, |cell| {
                    let value = cell.value_number().expect("Generated numeric input");
                    assert_eq!(value.fract(), 0.0);
                    sum(value as i64);
                })?;
                sheets += 1;
            }
        } else {
            let mut book = if mode.starts_with("umya-lazy") {
                umya_spreadsheet::reader::xlsx::lazy_read(input)?
            } else {
                umya_spreadsheet::reader::xlsx::read(input)?
            };
            sheets = book.sheet_count();
            if mode != "umya-lazy" {
                book.read_sheet_collection();
                for sheet in book.sheet_collection() {
                    // Use the supported unordered public collection API.
                    for cell in sheet.cells() {
                        let value = cell.value_number().ok_or("Expected numeric input")?;
                        if value.fract() != 0.0 {
                            return Err("Expected integer values".into());
                        }
                        sum(value as i64);
                    }
                }
                if mode.ends_with("edit") {
                    book.sheet_mut(0)?.cell_mut("A1").set_value_number(42);
                    let target = target.ok_or("Edit mode requires output")?;
                    umya_spreadsheet::writer::xlsx::write(&book, target)?;
                    output_bytes = std::fs::metadata(target)?.len();
                }
            }
        }
    } else if mode == "crabxl-stream" {
        let mut reader = WorkbookReader::open(input)?;
        let names: Vec<_> = reader
            .sheets()
            .iter()
            .map(|s| s.name().to_owned())
            .collect();
        for name in names {
            let mut rows = reader.rows(&name)?;
            let mut row = crabxl::Row::new(crabxl::RowIndex::new(0)?);
            while rows.read_row_into(&mut row)? {
                for cell in &row.cells {
                    let CellValue::Integer(value) = cell.value else {
                        return Err("Expected numeric input".into());
                    };
                    sum(value);
                }
            }
            sheets += 1;
        }
    } else if matches!(
        mode.as_str(),
        "crabxl-model" | "crabxl-edit" | "crabxl-lazy"
    ) {
        let mut book = LoadedWorkbook::with_options(
            File::open(input)?,
            LoadOptions {
                memory_policy: MemoryPolicy::Budget(1024 * 1024 * 1024),
                workbook: crabxl::WorkbookLimits {
                    max_bytes: 1024 * 1024 * 1024,
                    sheet: crabxl::EditLimits {
                        max_bytes: 512 * 1024 * 1024,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..LoadOptions::default()
            },
        )?;
        let ids: Vec<_> = book.model().sheets().map(|(id, _)| id).collect();
        sheets = ids.len();
        if mode != "crabxl-lazy" {
            for id in ids.iter().copied() {
                for cell in book.sheet(id)?.cells() {
                    let CellValue::Integer(value) = cell.value else {
                        return Err("Expected numeric input".into());
                    };
                    sum(value);
                }
            }
            if mode == "crabxl-edit" {
                book.set_value(ids[0], CellAddress::new(0, 0)?, CellValue::Integer(42))?;
                let target = target.ok_or("Edit mode requires output")?;
                book.save_path(target, crabxl::SaveOptions::default())?;
                output_bytes = std::fs::metadata(target)?.len();
            }
        }
    } else {
        return Err("Unknown comparison mode".into());
    }
    println!(
        "{{\"cells\":{count},\"checksum\":{checksum},\"sheets\":{sheets},\"output_bytes\":{output_bytes},\"operation_seconds\":{}}}",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
