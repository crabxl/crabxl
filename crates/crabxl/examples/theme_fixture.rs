//! Generate exact default/custom/omitted theme parts for public readback.
use crabxl::*;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: theme_fixture <output.xlsx> <default|custom|opaque|empty|omit>")?;
    let theme = match args.next().as_deref() {
        Some("default") => ThemeWritePolicy::ReferenceDefault,
        Some("custom") => ThemeWritePolicy::Validated(Theme::from_bytes(b"<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"custom\"><a:extLst/></a:theme>".to_vec().into_boxed_slice())),
        Some("opaque") => ThemeWritePolicy::Custom(Theme::from_bytes(b"not XML".to_vec().into_boxed_slice())),
        Some("empty") => ThemeWritePolicy::Custom(Theme::from_bytes(Box::default())),
        Some("omit") => ThemeWritePolicy::Omit,
        _ => return Err("Invalid theme mode".into()),
    };
    let mut writer = WorkbookWriter::new(WriteOptions {
        theme,
        ..Default::default()
    })?;
    writer.start_sheet("Sheet")?;
    writer.write_row(&Row {
        index: RowIndex::new(0)?,
        cells: vec![Cell {
            address: CellAddress::new(0, 0)?,
            value: CellValue::Integer(42),
            style: StyleId::new(0),
        }],
    })?;
    writer.finish(std::fs::File::create(&path)?)?;
    let mut book = WorkbookReader::open(&path)?;
    let raw_size = book.theme()?.map_or(0, |theme| theme.bytes().len());
    println!(
        "{{\"theme_bytes\":{raw_size},\"managed_bytes\":{}}}",
        book.theme_memory_bytes()
    );
    Ok(())
}
