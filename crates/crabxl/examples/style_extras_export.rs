//! Transfer typed differential/table catalogs into a new scalar package.
use crabxl::{WorkbookReader, WorkbookWriter, WriteOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("Missing input")?;
    let output = args.next().ok_or("Missing output")?;
    let count: usize = args.next().ok_or("Missing differential count")?.parse()?;
    let mut reader = WorkbookReader::open(input)?;
    let sheet = reader.read_sheet("Sheet")?;
    let theme = reader.theme()?.cloned();
    let catalog = reader.into_style_catalog()?.ok_or("Missing styles")?;
    if catalog.differential_styles.len() != count {
        return Err("Wrong differential count".into());
    }
    for (index, style) in catalog.differential_styles.iter().enumerate() {
        if style.font.as_ref().and_then(|font| font.name.as_deref())
            != Some(format!("Diff{index}").as_str())
        {
            return Err("Wrong differential font identity".into());
        }
    }
    let mut writer = WorkbookWriter::from_style_catalog(
        WriteOptions {
            max_styles: 100_000,
            theme: theme.map_or(
                crabxl::ThemeWritePolicy::ReferenceDefault,
                crabxl::ThemeWritePolicy::Custom,
            ),
            ..Default::default()
        },
        catalog,
    )?;
    writer.start_sheet("Sheet")?;
    for row in &sheet.rows {
        writer.write_row(row)?;
    }
    eprintln!("STYLE_BYTES {}", writer.style_memory_bytes());
    eprintln!("TEMP_BYTES {}", writer.stats().peak_temp_bytes);
    writer.finish(std::fs::File::create(output)?)?;
    println!("{count}");
    Ok(())
}
