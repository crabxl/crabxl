//! Exercise lazy existing-cell edits or unchanged part copying for measurements.
use crabxl::{CellAddress, CellValue, SaveOptions, WorkbookEditor};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let source = arguments
        .next()
        .ok_or("Usage: edit_demo <source.xlsx> <output.xlsx> [unchanged|edit|insert|verify]")?;
    let target = arguments.next().ok_or("Missing output path")?;
    let mode = arguments.next().unwrap_or_else(|| "edit".into());
    let mut editor = WorkbookEditor::open(source)?;
    if mode == "edit" {
        editor.set_value("Sheet", CellAddress::new(0, 0)?, CellValue::Integer(-1))?;
    } else if mode == "insert" {
        editor.upsert_value("Sheet", CellAddress::new(0, 10)?, CellValue::Integer(7))?;
        editor.upsert_value(
            "Sheet",
            CellAddress::new(1_000_000, 0)?,
            CellValue::Integer(9),
        )?;
    } else if !matches!(mode.as_str(), "unchanged" | "verify") {
        return Err("Unknown edit workload".into());
    }
    let stats = editor.save_path(
        target,
        SaveOptions {
            verify_unchanged: mode == "verify",
        },
    )?;
    println!(
        "{} {} {} {}",
        stats.copied_parts,
        stats.rewritten_parts,
        stats.rewritten_xml_bytes,
        editor.patch_bytes()
    );
    Ok(())
}
