//! Inspect formula records through the canonical public API.
use crabxl::{CellValue, FormulaType, ReadOptions, WorkbookReader};
fn source(flag: Option<&crabxl::FormulaFlag>) -> &str {
    flag.and_then(|value| value.source()).unwrap_or("")
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("Usage: formula_probe <file> [cached]")?;
    let cached = args.next().as_deref() == Some("cached");
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows_with_options(
        "Sheet",
        ReadOptions {
            data_only: cached,
            ..Default::default()
        },
    )?;
    while let Some(row) = rows.next_row()? {
        for cell in row.cells {
            print!("{}\t", cell.address);
            match cell.value {
                CellValue::Empty => println!("empty"),
                CellValue::Integer(value) => println!("integer\t{value}"),
                CellValue::Number(value) => println!("number\t{value}"),
                CellValue::Boolean(value) => println!("boolean\t{value}"),
                CellValue::Text(value) => println!("text\t{}", value.as_str()),
                CellValue::Error(value) => println!("error\t{}", value.as_str()),
                CellValue::Formula(value) => match value.formula_type() {
                    FormulaType::Array => {
                        let metadata = value.metadata().ok_or("Missing array metadata")?;
                        let reference = metadata.reference.as_ref().ok_or("Missing array range")?;
                        println!("array\t{}\t={}", reference.spelling(), value.expression());
                    }
                    FormulaType::DataTable => {
                        let metadata = value.metadata().ok_or("Missing table metadata")?;
                        let reference = metadata.reference.as_ref().ok_or("Missing table range")?;
                        let table = metadata.data_table.as_ref().ok_or("Missing table fields")?;
                        println!(
                            "table\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                            reference.spelling(),
                            source(metadata.flags.calculate_cell.as_ref()),
                            source(table.two_dimensions.as_ref()),
                            source(table.row_table.as_ref()),
                            table.input1.as_deref().unwrap_or(""),
                            table.input2.as_deref().unwrap_or(""),
                            source(table.deleted1.as_ref()),
                            source(table.deleted2.as_ref())
                        );
                    }
                    _ => println!("text\t={}", value.expression()),
                },
                other => return Err(format!("Unexpected probe value: {other:?}").into()),
            }
        }
    }
    Ok(())
}
