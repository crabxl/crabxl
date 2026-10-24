//! Create structured formula records for independent public-reference readback.
use crabxl::{
    Cell, CellAddress, CellValue, DataTableOptions, Formula, FormulaFlags, FormulaMetadata,
    FormulaRange, FormulaType, Row, RowIndex, StyleId, WorkbookWriter, WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: structured_formula_fixture <file>")?;
    let array = Formula::with_metadata(
        "=SUM(C1:C2)",
        Some(CellValue::Integer(5)),
        FormulaMetadata {
            kind: FormulaType::Array,
            reference: Some(FormulaRange::from_xml("$A$1:$B$2")?),
            flags: FormulaFlags {
                always_calculate: Some(false.into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    let table = Formula::with_metadata(
        "",
        Some(CellValue::Integer(0)),
        FormulaMetadata {
            kind: FormulaType::DataTable,
            literal_array_text: false,
            annotations: None,
            reference: Some(FormulaRange::from_xml("D1:E2")?),
            flags: FormulaFlags {
                calculate_cell: Some(false.into()),
                ..Default::default()
            },
            data_table: Some(Box::new(DataTableOptions {
                two_dimensions: Some(true.into()),
                row_table: Some(false.into()),
                input1: Some("$A$1".into()),
                input2: Some("B1".into()),
                deleted1: Some(false.into()),
                deleted2: Some(true.into()),
            })),
        },
    )?;
    let empty_input_table = Formula::with_metadata(
        "",
        None,
        FormulaMetadata {
            kind: FormulaType::DataTable,
            reference: Some(FormulaRange::from_xml("I1:J2")?),
            data_table: Some(Box::new(DataTableOptions {
                two_dimensions: Some(false.into()),
                row_table: Some(false.into()),
                input1: Some("".into()),
                ..Default::default()
            })),
            ..Default::default()
        },
    )?;
    let formulas = [
        (0, array),
        (3, table),
        (5, Formula::from_source("=1", None, None)?),
        (6, Formula::from_source("", None, None)?),
        (8, empty_input_table),
    ];
    let mut row = Row::new(RowIndex::new(0)?);
    for (column, formula) in formulas {
        row.cells.push(Cell {
            address: CellAddress::new(0, column)?,
            value: CellValue::Formula(Box::new(formula)),
            style: StyleId::new(0),
        });
    }
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sheet")?;
    writer.write_row(&row)?;
    writer.finish(std::fs::File::create(path)?)?;
    Ok(())
}
