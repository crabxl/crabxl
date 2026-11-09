//! One benchmark case per process, so that the parent can measure wall time and
//! peak memory of exactly one library doing one task.
//!
//! ```text
//! driver fixture <rows> <output>
//! driver <library> read  <input>
//! driver <library> write <rows> <output>
//! driver <library> edit  <input> <output> <rows in input>
//! ```
//!
//! Every case prints the number of cells it handled, so the parent can check
//! that all libraries did the same work.
use std::{error::Error, fs::File, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Columns of the generated sheet. Every row has the same 10 typed cells.
const COLUMNS: u32 = 10;
const SHEET: &str = "Data";
/// Rows changed by the edit scenario: one in every `EDIT_STEP`.
const EDIT_STEP: u32 = 100;
/// Rows appended by the edit scenario.
const EDIT_APPEND: u32 = 100;

/// One generated value. The same data shape is used by the Python suite.
enum Value {
    Int(i64),
    Float(f64),
    Text(String),
    Date(i32, u32, u32),
    Bool(bool),
}

fn value(row: u32, column: u32) -> Value {
    match column {
        0 => Value::Int(i64::from(row)),
        1 => Value::Float(f64::from(row) * 1.5),
        2 => Value::Text(format!("item-{}", row % 1000)),
        3 => Value::Text(format!("row {row} unique text")),
        4 => {
            let day = row % 3650;
            Value::Date(
                2000 + (day / 365) as i32,
                1 + (day % 365) / 31 % 12,
                1 + day % 28,
            )
        }
        5 => Value::Bool(row.is_multiple_of(2)),
        _ => Value::Float(f64::from(row * column) / 7.0),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |index: usize| -> Result<&str> {
        args.get(index)
            .map(String::as_str)
            .ok_or_else(|| "Missing argument; see the driver documentation".into())
    };
    let cells = match (arg(0)?, arg(1).unwrap_or("")) {
        ("fixture", _) => {
            let rows = arg(1)?.parse()?;
            rust_xlsxwriter_write(rows, Path::new(arg(2)?), false)?
        }
        (library, "read") => read(library, Path::new(arg(2)?))?,
        (library, "write") => write(library, arg(2)?.parse()?, Path::new(arg(3)?))?,
        (library, "edit") => edit(
            library,
            Path::new(arg(2)?),
            Path::new(arg(3)?),
            arg(4)?.parse()?,
        )?,
        _ => return Err("Unknown command".into()),
    };
    println!("{cells}");
    Ok(())
}

// ---------------------------------------------------------------- read

fn read(library: &str, input: &Path) -> Result<u64> {
    match library {
        "crabxl" => {
            use crabxl::CellValue;
            let mut reader = crabxl::WorkbookReader::open(input)?;
            let mut rows = reader.rows(SHEET)?;
            let mut row = crabxl::Row::new(crabxl::RowIndex::new(0)?);
            let mut cells = 0;
            while rows.read_row_into(&mut row)? {
                for cell in &row.cells {
                    // Touch every value so that no decoding is skipped.
                    std::hint::black_box(match &cell.value {
                        CellValue::Text(text) => text.as_str().len(),
                        CellValue::Empty => 0,
                        _ => 1,
                    });
                    cells += 1;
                }
            }
            Ok(cells)
        }
        "crabxl-model" => {
            use crabxl::CellValue;
            let budget = 8 * 1024 * 1024 * 1024;
            let mut book =
                crabxl::LoadedWorkbook::with_options(File::open(input)?, load_options(budget))?;
            let id = book.sheet_id(SHEET).ok_or("Missing sheet")?;
            let mut cells = 0;
            for cell in book.sheet(id)?.cells() {
                std::hint::black_box(match &cell.value {
                    CellValue::Text(text) => text.as_str().len(),
                    _ => 1,
                });
                cells += 1;
            }
            Ok(cells)
        }
        "calamine-stream" => {
            let mut book: calamine::Xlsx<_> = calamine::open_workbook(input)?;
            let mut reader = book.worksheet_cells_reader(SHEET)?;
            let mut cells = 0;
            while let Some(cell) = reader.next_cell()? {
                std::hint::black_box(cell.get_value());
                cells += 1;
            }
            Ok(cells)
        }
        "calamine" => {
            use calamine::Reader;
            let mut book: calamine::Xlsx<_> = calamine::open_workbook(input)?;
            let range = book.worksheet_range(SHEET)?;
            let mut cells = 0;
            for row in range.rows() {
                for value in row {
                    std::hint::black_box(value);
                    cells += 1;
                }
            }
            Ok(cells)
        }
        "umya" => {
            let book = umya_spreadsheet::reader::xlsx::read(input)?;
            let sheet = book.sheet_by_name(SHEET)?;
            let mut cells = 0;
            for cell in sheet.cells() {
                std::hint::black_box(cell.value());
                cells += 1;
            }
            Ok(cells)
        }
        "oxlsx" => {
            let book = oxlsx::Workbook::open_readonly(input)?;
            let sheet = book.sheet(0).ok_or("Missing sheet")?;
            let mut cells = 0;
            for row in sheet.rows() {
                for cell in row {
                    std::hint::black_box(&cell);
                    cells += 1;
                }
            }
            Ok(cells)
        }
        _ => Err(format!("{library} cannot read").into()),
    }
}

// ---------------------------------------------------------------- write

fn write(library: &str, rows: u32, output: &Path) -> Result<u64> {
    match library {
        "crabxl" => crabxl_write(rows, output),
        "rust_xlsxwriter" => rust_xlsxwriter_write(rows, output, false),
        "rust_xlsxwriter-constant-memory" => rust_xlsxwriter_write(rows, output, true),
        "umya" => umya_write(rows, output),
        "oxlsx" => {
            let mut book = oxlsx::Workbook::new();
            let sheet = book.active_mut().ok_or("Missing sheet")?;
            for row in 0..rows {
                sheet.append((0..COLUMNS).map(|column| oxlsx_value(value(row, column))));
            }
            book.save(output)?;
            Ok(u64::from(rows) * u64::from(COLUMNS))
        }
        _ => Err(format!("{library} cannot write").into()),
    }
}

fn crabxl_value(value: Value) -> Result<crabxl::CellValue> {
    use crabxl::CellValue;
    Ok(match value {
        Value::Int(number) => CellValue::Integer(number),
        Value::Float(number) => CellValue::Number(number),
        Value::Text(text) => CellValue::text(text.into_boxed_str()),
        Value::Date(year, month, day) => {
            CellValue::DateTime(Box::new(crabxl::ExcelDateTime::from_ymd(year, month, day)?))
        }
        Value::Bool(flag) => CellValue::Boolean(flag),
    })
}

fn crabxl_write(rows: u32, output: &Path) -> Result<u64> {
    let mut writer = crabxl::WorkbookWriter::new(crabxl::WriteOptions::default())?;
    writer.start_sheet(SHEET)?;
    let mut cells = crabxl::Row::new(crabxl::RowIndex::new(0)?);
    for row in 0..rows {
        cells.index = crabxl::RowIndex::new(row)?;
        cells.cells.clear();
        for column in 0..COLUMNS {
            cells.cells.push(crabxl::Cell {
                address: crabxl::CellAddress::new(row, column)?,
                value: crabxl_value(value(row, column))?,
                style: crabxl::StyleId::new(0),
            });
        }
        writer.write_row(&cells)?;
    }
    writer.close_sheet()?;
    writer.finish(File::create(output)?)?;
    Ok(u64::from(rows) * u64::from(COLUMNS))
}

fn rust_xlsxwriter_write(rows: u32, output: &Path, constant_memory: bool) -> Result<u64> {
    let mut book = rust_xlsxwriter::Workbook::new();
    let date = rust_xlsxwriter::Format::new().set_num_format("yyyy-mm-dd");
    let sheet = if constant_memory {
        book.add_worksheet_with_constant_memory()
    } else {
        book.add_worksheet()
    };
    sheet.set_name(SHEET)?;
    for row in 0..rows {
        for column in 0..COLUMNS {
            let col = column as u16;
            match value(row, column) {
                Value::Int(number) => sheet.write_number(row, col, number as f64)?,
                Value::Float(number) => sheet.write_number(row, col, number)?,
                Value::Text(text) => sheet.write_string(row, col, text)?,
                Value::Date(year, month, day) => {
                    let when = rust_xlsxwriter::ExcelDateTime::from_ymd(
                        year as u16,
                        month as u8,
                        day as u8,
                    )?;
                    sheet.write_datetime_with_format(row, col, &when, &date)?
                }
                Value::Bool(flag) => sheet.write_boolean(row, col, flag)?,
            };
        }
    }
    book.save(output)?;
    Ok(u64::from(rows) * u64::from(COLUMNS))
}

fn oxlsx_value(value: Value) -> oxlsx::Value {
    match value {
        Value::Int(number) => oxlsx::Value::Number(number as f64),
        Value::Float(number) => oxlsx::Value::Number(number),
        Value::Text(text) => oxlsx::Value::String(text),
        Value::Date(year, month, day) => oxlsx::Value::Date(
            chrono::NaiveDate::from_ymd_opt(year, month, day).expect("Generated dates are valid"),
        ),
        Value::Bool(flag) => oxlsx::Value::Bool(flag),
    }
}

/// Serial day number of a date in the 1900 date system, for umya.
fn serial(year: i32, month: u32, day: u32) -> Result<f64> {
    Ok(crabxl::ExcelDateTime::from_ymd(year, month, day)?.serial())
}

fn umya_write(rows: u32, output: &Path) -> Result<u64> {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0)?;
    sheet.set_name(SHEET);
    for row in 0..rows {
        for column in 0..COLUMNS {
            let cell = sheet.cell_mut((column + 1, row + 1));
            match value(row, column) {
                Value::Int(number) => cell.set_value_number(number as f64),
                Value::Float(number) => cell.set_value_number(number),
                Value::Text(text) => cell.set_value_string(text),
                Value::Date(year, month, day) => {
                    cell.set_value_number(serial(year, month, day)?);
                    cell.style_mut()
                        .number_format_mut()
                        .set_format_code("yyyy-mm-dd");
                    cell
                }
                Value::Bool(flag) => cell.set_value_bool(flag),
            };
        }
    }
    umya_spreadsheet::writer::xlsx::write(&book, output)?;
    Ok(u64::from(rows) * u64::from(COLUMNS))
}

/// Allow up to `budget` bytes, so that the large size fits. CrabXL's default
/// memory limit is lower and would reject it.
fn load_options(budget: usize) -> crabxl::LoadOptions {
    crabxl::LoadOptions {
        memory_policy: crabxl::MemoryPolicy::Budget(budget),
        resources: crabxl::ResourceLimits {
            max_materialized_bytes: budget,
            ..Default::default()
        },
        workbook: crabxl::WorkbookLimits {
            max_bytes: budget,
            sheet: crabxl::EditLimits {
                max_bytes: budget,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------- edit

/// Load the fixture, change column B in one row of every `EDIT_STEP`, append
/// `EDIT_APPEND` rows and save. Returns the number of changed cells.
fn edit(library: &str, input: &Path, output: &Path, rows: u32) -> Result<u64> {
    match library {
        "crabxl" => {
            let budget = 8 * 1024 * 1024 * 1024;
            let mut book =
                crabxl::LoadedWorkbook::with_options(File::open(input)?, load_options(budget))?;
            let id = book.sheet_id(SHEET).ok_or("Missing sheet")?;
            let mut changed = 0;
            for row in (0..rows).step_by(EDIT_STEP as usize) {
                let address = crabxl::CellAddress::new(row, 1)?;
                book.set_value(id, address, crabxl::CellValue::text("changed"))?;
                changed += 1;
            }
            for row in rows..rows + EDIT_APPEND {
                let values = (0..COLUMNS)
                    .map(|column| crabxl_value(value(row, column)))
                    .collect::<Result<Vec<_>>>()?;
                book.append(id, values)?;
                changed += u64::from(COLUMNS);
            }
            book.save_path(output, crabxl::SaveOptions::default())?;
            Ok(changed)
        }
        "umya" => {
            let mut book = umya_spreadsheet::reader::xlsx::read(input)?;
            let sheet = book.sheet_by_name_mut(SHEET)?;
            let mut changed = 0;
            for row in (1..=rows).step_by(EDIT_STEP as usize) {
                sheet.cell_mut((2, row)).set_value_string("changed");
                changed += 1;
            }
            for row in rows..rows + EDIT_APPEND {
                for column in 0..COLUMNS {
                    let cell = sheet.cell_mut((column + 1, row + 1));
                    match value(row, column) {
                        Value::Int(number) => cell.set_value_number(number as f64),
                        Value::Float(number) => cell.set_value_number(number),
                        Value::Text(text) => cell.set_value_string(text),
                        Value::Date(year, month, day) => {
                            cell.set_value_number(serial(year, month, day)?)
                        }
                        Value::Bool(flag) => cell.set_value_bool(flag),
                    };
                    changed += 1;
                }
            }
            umya_spreadsheet::writer::xlsx::write(&book, output)?;
            Ok(changed)
        }
        "oxlsx" => {
            let mut book = oxlsx::Workbook::open(input)?;
            let sheet = book.sheet_mut(0).ok_or("Missing sheet")?;
            let mut changed = 0;
            for row in (1..=rows).step_by(EDIT_STEP as usize) {
                sheet.set_cell(&format!("B{row}"), "changed");
                changed += 1;
            }
            for row in rows..rows + EDIT_APPEND {
                sheet.append((0..COLUMNS).map(|column| oxlsx_value(value(row, column))));
                changed += u64::from(COLUMNS);
            }
            book.save(output)?;
            Ok(changed)
        }
        _ => Err(format!("{library} cannot edit").into()),
    }
}
