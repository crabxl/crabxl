//! Canonical public formula-reference strings without eager coordinate parsing.
use crabxl::{
    Cell, CellAddress, CellValue, DataTableOptions, Formula, FormulaMetadata, FormulaReference,
    FormulaType, Row, RowIndex, StyleId, WorkbookReader, WorkbookWriter, WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing output")?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    let references = ["", "A1:B2", "$A$1:$B$2", "Sheet1!A1:B2", "not a range"];
    for (column, reference) in references.into_iter().enumerate() {
        let formula = Formula::from_array_text(
            Some("=1".into()),
            None,
            FormulaMetadata {
                kind: FormulaType::Array,
                reference: Some(FormulaReference::from_literal(reference)),
                ..Default::default()
            },
        )?;
        row.cells.push(Cell {
            address: CellAddress::new(0, column as u32)?,
            value: CellValue::Formula(Box::new(formula)),
            style: StyleId::new(0),
        });
    }
    for (index, input) in ["", "C1", "Sheet1!C1", "input"].into_iter().enumerate() {
        let formula = Formula::with_optional_expression(
            None,
            None,
            FormulaMetadata {
                kind: FormulaType::DataTable,
                reference: Some(FormulaReference::from_literal("opaque")),
                data_table: Some(Box::new(DataTableOptions {
                    input1: Some(input.into()),
                    input2: Some("".into()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )?;
        row.cells.push(Cell {
            address: CellAddress::new(0, index as u32 + 5)?,
            value: CellValue::Formula(Box::new(formula)),
            style: StyleId::new(0),
        });
    }
    writer.write_row(&row)?;
    writer.finish(std::fs::File::create(&path)?)?;
    let mut reader = WorkbookReader::new(std::fs::File::open(path)?)?;
    let loaded = reader.read_sheet("Sheet")?;
    for (index, cell) in loaded.rows[0].cells.iter().enumerate() {
        let CellValue::Formula(formula) = &cell.value else {
            return Err("Lost formula".into());
        };
        let metadata = formula.metadata().ok_or("Lost metadata")?;
        let expected = if index < 5 {
            references[index]
        } else {
            "opaque"
        };
        if metadata
            .reference
            .as_ref()
            .map(|value| value.spelling())
            .as_deref()
            != if expected.is_empty() {
                None
            } else {
                Some(expected)
            }
        {
            return Err("Changed literal reference".into());
        }
    }
    println!("9");
    Ok(())
}
