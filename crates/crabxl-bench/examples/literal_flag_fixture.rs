//! Canonical literal data-table flags with public-compatible output spelling.
use crabxl::{
    Cell, CellAddress, CellValue, DataTableOptions, Formula, FormulaFlag, FormulaFlags,
    FormulaMetadata, FormulaReference, FormulaType, Row, RowIndex, StyleId, WorkbookWriter,
    WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing output")?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    for (column, literal) in ["", "false", "invalid", "0", " true "]
        .into_iter()
        .enumerate()
    {
        let flag = FormulaFlag::from_literal(literal);
        let formula = Formula::with_optional_expression(
            None,
            None,
            FormulaMetadata {
                kind: FormulaType::DataTable,
                reference: Some(FormulaReference::from_literal("A1:B2")),
                flags: FormulaFlags {
                    calculate_cell: Some(flag.clone()),
                    ..Default::default()
                },
                data_table: Some(Box::new(DataTableOptions {
                    two_dimensions: Some(flag.clone()),
                    row_table: Some(flag.clone()),
                    deleted1: Some(flag.clone()),
                    deleted2: Some(flag),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )?;
        row.cells.push(Cell {
            address: CellAddress::new(0, column as u32)?,
            value: CellValue::Formula(Box::new(formula)),
            style: StyleId::new(0),
        });
    }
    writer.write_row(&row)?;
    writer.finish(std::fs::File::create(path)?)?;
    println!("5");
    Ok(())
}
