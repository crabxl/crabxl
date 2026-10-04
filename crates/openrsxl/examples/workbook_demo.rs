//! Measure aggregate owned models, explicit copies and borrowed XLSX export.
use openrsxl::{
    Cell, CellAddress, CellValue, EditLimits, StyleId, Workbook, WorkbookLimits, WorkbookWriter,
    WriteOptions,
};
use std::{fs::File, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let rows = args
        .get(1)
        .ok_or("Usage: workbook_demo <rows> <output.xlsx>")?
        .parse::<u32>()?;
    let output = args.get(2).ok_or("Missing output path")?;
    let mut book = Workbook::new(WorkbookLimits {
        max_bytes: 1024 * 1024 * 1024,
        max_cells: 4_000_000,
        sheet: EditLimits {
            max_bytes: 512 * 1024 * 1024,
            max_cells: 2_000_000,
        },
        ..WorkbookLimits::default()
    })?;
    let original = book.create_sheet("Original")?;
    let begin = Instant::now();
    {
        let mut sheet = book.sheet_mut(original)?;
        for row in 0..rows {
            for column in 0..10 {
                sheet.set(Cell {
                    address: CellAddress::new(row, column)?,
                    value: CellValue::Integer(i64::from(row) * 10 + i64::from(column)),
                    style: StyleId::new(0),
                })?;
            }
        }
    }
    let build_seconds = begin.elapsed().as_secs_f64();
    let begin = Instant::now();
    let copied = book.copy_sheet(original, "Copy")?;
    let copy_seconds = begin.elapsed().as_secs_f64();
    book.move_sheet(copied, 0)?;
    book.rename_sheet(original, "Renamed")?;
    book.set_active_sheet(original)?;
    let begin = Instant::now();
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.write_workbook(&book)?;
    writer.finish(File::create(output)?)?;
    let write_seconds = begin.elapsed().as_secs_f64();
    let sum = book
        .sheets()
        .flat_map(|(_, sheet)| sheet.cells())
        .try_fold(0i64, |sum, cell| match cell.value {
            CellValue::Integer(value) => sum.checked_add(value).ok_or("Checksum overflow"),
            _ => Err("Unexpected cell type"),
        })?;
    println!(
        "{{\"cells\":{},\"sum\":{},\"charged_bytes\":{},\"build_seconds\":{},\"copy_seconds\":{},\"write_seconds\":{}}}",
        book.cell_count(),
        sum,
        book.charged_bytes(),
        build_seconds,
        copy_seconds,
        write_seconds
    );
    Ok(())
}
