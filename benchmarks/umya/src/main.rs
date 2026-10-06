//! Public-API comparison of editable Rust models and numeric cell streams.
use crabxl::{CellAddress, CellValue, LoadOptions, LoadedWorkbook, MemoryPolicy, WorkbookReader};
use std::{fs::File, path::Path, time::Instant};

fn verify_text(text: &str, index: usize, unique: usize) -> Result<i64, Box<dyn std::error::Error>> {
    if unique == 0 || text != format!("item-{:08}-{}", index % unique, "x".repeat(96)) {
        return Err("Text model value/order mismatch".into());
    }
    Ok(text.len() as i64)
}

fn verify_styled(cell: &crabxl::Cell, index: usize) -> Result<(), Box<dyn std::error::Error>> {
    let row = index / 10;
    let column = index % 10;
    if cell.address.row.get() as usize != row || cell.address.column.get() as usize != column {
        return Err("Styled model coordinate mismatch".into());
    }
    let expected_style = match column {
        1 | 2 | 4 | 7 => 1,
        3 => 2,
        _ => 0,
    };
    if cell.style.get() != expected_style {
        return Err("Styled model identity mismatch".into());
    }
    let valid = match (column, &cell.value) {
        (0 | 9, CellValue::Number(value)) => *value == 1.25,
        (1 | 7, CellValue::DateTime(value)) => {
            value.kind() == crabxl::DateKind::DateTime
                && value.serial() == 45292.25 + (row % 365) as f64
        }
        (2, CellValue::DateTime(value)) => {
            value.kind() == crabxl::DateKind::Time && value.serial() == 0.5
        }
        (3, CellValue::DateTime(value)) => {
            value.kind() == crabxl::DateKind::Duration && value.serial() == (row % 101) as f64 / 4.0
        }
        (4, CellValue::Boolean(value)) => *value == !row.is_multiple_of(2),
        (5, CellValue::Text(value)) => value.as_str() == format!("styled-{row:08}"),
        (6, CellValue::Error(value)) => value.as_str() == "#DIV/0!",
        (8, CellValue::Integer(value)) => *value == row as i64,
        _ => false,
    };
    if !valid {
        return Err("Styled model value mismatch".into());
    }
    Ok(())
}

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
    if mode.starts_with("write-text-") {
        let rows = args
            .get(4)
            .ok_or("Missing text row count")?
            .parse::<u32>()?;
        let output = target.ok_or("Missing text output path")?;
        let value = |row: u32, column: u32| {
            format!("{row}:{column}:")
                + &"plain \u{6587}\u{5b57} caf\u{e9} &<> \"'\t\r\n ".repeat(4)
        };
        if mode == "write-text-crabxl-stream" {
            let mut writer = crabxl::WorkbookWriter::new(crabxl::WriteOptions::default())?;
            writer.start_sheet("Sheet")?;
            let mut cells = crabxl::Row::new(crabxl::RowIndex::new(0)?);
            for row in 0..rows {
                cells.index = crabxl::RowIndex::new(row)?;
                cells.cells.clear();
                for column in 0..10 {
                    let text = value(row, column);
                    sum(text.len() as i64);
                    cells.cells.push(crabxl::Cell {
                        address: CellAddress::new(row, column)?,
                        value: CellValue::text(text.into_boxed_str()),
                        style: crabxl::StyleId::new(0),
                    });
                }
                writer.write_row(&cells)?;
            }
            writer.finish(File::create(output)?)?;
        } else if mode == "write-text-crabxl-model" {
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
                        let text = value(row, column);
                        sum(text.len() as i64);
                        sheet.set(crabxl::Cell {
                            address: CellAddress::new(row, column)?,
                            value: CellValue::text(text.into_boxed_str()),
                            style: crabxl::StyleId::new(0),
                        })?;
                    }
                }
            }
            let mut writer = crabxl::WorkbookWriter::new(crabxl::WriteOptions::default())?;
            writer.write_workbook(&book)?;
            writer.finish(File::create(output)?)?;
        } else if matches!(
            mode.as_str(),
            "write-text-rust_xlsxwriter-normal" | "write-text-rust_xlsxwriter-constant"
        ) {
            let mut book = rust_xlsxwriter::Workbook::new();
            let sheet = if mode.ends_with("constant") {
                book.add_worksheet_with_constant_memory()
            } else {
                book.add_worksheet()
            };
            sheet.set_name("Sheet")?;
            for row in 0..rows {
                for column in 0..10 {
                    let text = value(row, column);
                    sum(text.len() as i64);
                    sheet.write_string(row, column as u16, text)?;
                }
            }
            book.save(output)?;
        } else {
            return Err("Unknown text write mode".into());
        }
        sheets = 1;
        output_bytes = std::fs::metadata(output)?.len();
    } else if mode.starts_with("write-") {
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
        } else if matches!(
            mode.as_str(),
            "write-crabxl-model" | "write-crabxl-reverse-model"
        ) {
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
                for row_offset in 0..rows {
                    let row = if mode == "write-crabxl-reverse-model" {
                        rows - row_offset - 1
                    } else {
                        row_offset
                    };
                    for column_offset in 0..10 {
                        let column = if mode == "write-crabxl-reverse-model" {
                            9 - column_offset
                        } else {
                            column_offset
                        };
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
    } else if mode == "calamine-text-model" {
        use calamine::{Data, Reader};
        let unique: usize = args.get(4).ok_or("Missing unique count")?.parse()?;
        let mut book: calamine::Xlsx<_> = calamine::open_workbook(input)?;
        let mut ranges = Vec::new();
        for name in book.sheet_names() {
            let range = book.worksheet_range(&name)?;
            for (_, _, value) in range.used_cells() {
                let Data::String(text) = value else {
                    return Err("Expected text".into());
                };
                checksum += verify_text(text, count, unique)?;
                count += 1;
            }
            sheets += 1;
            ranges.push(range);
        }
        std::hint::black_box(&ranges);
    } else if mode.starts_with("calamine-") {
        use calamine::{Data, DataRef, Reader};
        let mut book: calamine::Xlsx<_> = calamine::open_workbook(input)?;
        let names = book.sheet_names();
        sheets = names.len();
        let mut retained_ranges = Vec::new();
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
            } else if mode == "calamine-range" || mode == "calamine-model" {
                let range = book.worksheet_range(&name)?;
                for cell in range.used_cells() {
                    let value = match cell.2 {
                        Data::Int(value) => *value,
                        Data::Float(value) if value.fract() == 0.0 => *value as i64,
                        _ => return Err("Expected numeric input".into()),
                    };
                    sum(value);
                }
                if mode == "calamine-model" {
                    retained_ranges.push(range);
                }
            } else {
                return Err("Unknown calamine mode".into());
            }
        }
        // Explicitly keep all returned ranges alive through complete traversal.
        std::hint::black_box(&retained_ranges);
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
    } else if mode == "crabxl-text-model" || mode == "crabxl-styled-model" {
        let unique: usize = args.get(4).ok_or("Missing unique count")?.parse()?;
        let storage = match args.get(5).map(String::as_str).unwrap_or("memory") {
            "memory" => crabxl::SharedStringStorage::Memory,
            "disk" => crabxl::SharedStringStorage::Disk,
            _ => return Err("Invalid SST policy".into()),
        };
        let maximum = 1024 * 1024 * 1024;
        let mut book = LoadedWorkbook::with_options(
            File::open(input)?,
            LoadOptions {
                memory_policy: MemoryPolicy::Budget(maximum),
                resources: crabxl::ResourceLimits {
                    max_materialized_bytes: maximum,
                    ..Default::default()
                },
                workbook: crabxl::WorkbookLimits {
                    max_bytes: maximum,
                    sheet: crabxl::EditLimits {
                        max_bytes: maximum,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                read: crabxl::ReadOptions {
                    data_only: true,
                    ..Default::default()
                },
                shared_strings: crabxl::SharedStringOptions {
                    storage,
                    memory_policy: MemoryPolicy::Budget(256 * 1024 * 1024),
                    cache_bytes: 1024 * 1024,
                    temp_directory: target.and_then(Path::parent).map(Path::to_owned),
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let ids = book.model().sheets().map(|(id, _)| id).collect::<Vec<_>>();
        for id in ids {
            for cell in book.sheet(id)?.cells() {
                if mode == "crabxl-text-model" {
                    let CellValue::Text(text) = &cell.value else {
                        return Err("Expected text".into());
                    };
                    checksum += verify_text(text.as_str(), count, unique)?;
                } else {
                    verify_styled(cell, count)?;
                    checksum += 1;
                }
                count += 1;
            }
            sheets += 1;
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
