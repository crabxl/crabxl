//! Canonical absent/empty array expression creation and source-presence readback.
use crabxl::{
    Cell, CellAddress, CellValue, Formula, FormulaMetadata, FormulaType, Row, RowIndex, StyleId,
    WorkbookWriter, WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing output")?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    for (column, expression) in [None, Some(""), Some("=1")].into_iter().enumerate() {
        let formula = Formula::with_optional_expression(
            expression.map(Box::<str>::from),
            None,
            FormulaMetadata {
                kind: FormulaType::Array,
                ..Default::default()
            },
        )?;
        if formula.optional_expression()
            != expression.map(|value| value.strip_prefix('=').unwrap_or(value))
        {
            return Err("Lost expression presence".into());
        }
        row.cells.push(Cell {
            address: CellAddress::new(0, column as u32)?,
            value: CellValue::Formula(Box::new(formula)),
            style: StyleId::new(0),
        });
    }
    writer.write_row(&row)?;
    writer.finish(std::fs::File::create(path)?)?;
    println!("3");
    Ok(())
}
