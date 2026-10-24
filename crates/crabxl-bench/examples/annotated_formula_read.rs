//! Verify explicit dynamic formula reference retention without decoding scalar annotations.
use crabxl::{
    CellMetadataReadPolicy, CellValue, ColumnIndex, FormulaType, ReadOptions, Row, RowIndex,
    WorkbookReader,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: annotated_formula_read <file>")?;
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows_with_options(
        "Sheet",
        ReadOptions {
            columns: Some(ColumnIndex::new(0)?..=ColumnIndex::new(0)?),
            cell_metadata_policy: CellMetadataReadPolicy::RetainFormulaReferences,
            ..Default::default()
        },
    )?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut count = 0u64;
    while rows.read_row_into(&mut row)? {
        let index = row.index.get() + 1;
        if row.cells.len() != 1 {
            return Err("Wrong projected formula width".into());
        }
        let CellValue::Formula(formula) = &row.cells[0].value else {
            return Err("Missing formula".into());
        };
        let metadata = formula.metadata().ok_or("Missing formula metadata")?;
        let annotation = metadata.annotations.as_ref().ok_or("Missing annotation")?;
        if formula.formula_type() != FormulaType::Array
            || formula.expression() != "_xlfn.SEQUENCE(1)"
            || formula.cached() != Some(&CellValue::Integer(i64::from(index)))
            || metadata.reference.as_ref().map(|range| range.spelling())
                != Some(format!("A{index}").into())
            || annotation.cell_metadata.as_deref() != Some("1")
            || annotation.value_metadata.is_some()
        {
            return Err("Incorrect formula/cache/reference".into());
        }
        count += 1;
    }
    if rows.projected_metadata_cells() != 0 {
        return Err("Retention was counted as discarded projection".into());
    }
    println!("{count}");
    Ok(())
}
