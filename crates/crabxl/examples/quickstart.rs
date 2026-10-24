//! Create, stream, edit and re-read a small workbook.
//!
//! Run with `cargo run -p crabxl --example quickstart [output directory]`.
//! The files are written to the system temporary directory by default.
use crabxl::{
    Cell, CellAddress, CellValue, Formula, LoadedWorkbook, Row, RowIndex, SaveOptions, StyleId,
    WorkbookEditor, WorkbookReader, WorkbookWriter, WriteOptions,
};
use std::{fs::File, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let created = directory.join("crabxl-quickstart.xlsx");
    let patched = directory.join("crabxl-quickstart-patched.xlsx");
    let edited = directory.join("crabxl-quickstart-edited.xlsx");

    // 1. Create a workbook sequentially. Rows are written in ascending order.
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sales")?;
    let records: [(&str, i64); 3] = [("North", 120), ("South", 95), ("West", 143)];
    for (index, (region, amount)) in records.into_iter().enumerate() {
        let row_index = u32::try_from(index)?;
        let mut row = Row::new(RowIndex::new(row_index)?);
        row.cells.push(Cell {
            address: CellAddress::new(row_index, 0)?,
            value: CellValue::text(region),
            style: StyleId::new(0),
        });
        row.cells.push(Cell {
            address: CellAddress::new(row_index, 1)?,
            value: CellValue::Integer(amount),
            style: StyleId::new(0),
        });
        writer.write_row(&row)?;
    }
    let mut total = Row::new(RowIndex::new(3)?);
    total.cells.push(Cell {
        address: "B4".parse()?,
        value: CellValue::Formula(Box::new(Formula::new("=SUM(B1:B3)", None)?)),
        style: StyleId::new(0),
    });
    writer.write_row(&total)?;
    writer.close_sheet()?;
    writer.finish(File::create(&created)?)?;

    // 2. Stream the rows back. Only physically present cells are returned.
    let mut reader = WorkbookReader::open(&created)?;
    let mut rows = reader.rows("Sales")?;
    while let Some(row) = rows.next_row()? {
        for cell in row.cells {
            println!("{}: {:?}", cell.address, cell.value);
        }
    }

    // 3. Patch one cell in place. Untouched package parts are copied as-is.
    let mut editor = WorkbookEditor::open(&created)?;
    editor.set_value("Sales", "B2".parse()?, CellValue::Integer(101))?;
    editor.save_path(&patched, SaveOptions::default())?;

    // 4. Load an editable model for structural changes such as appending rows.
    let mut workbook = LoadedWorkbook::open(&patched)?;
    let sales = workbook.sheet_id("Sales").ok_or("Missing Sales sheet")?;
    workbook.append(sales, vec![CellValue::text("East"), CellValue::Integer(88)])?;
    workbook.save_path(&edited, SaveOptions::default())?;

    println!("Wrote {}", edited.display());
    Ok(())
}
