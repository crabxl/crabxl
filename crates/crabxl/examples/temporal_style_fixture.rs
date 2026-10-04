//! Shared component-preserving temporal assignment across existing number formats.
use crabxl::{
    Alignment, Border, BorderLine, BorderSide, Cell, CellAddress, CellStyle, CellValue, Color,
    ColorKind, ExcelDateTime, Fill, FillPattern, Font, PatternFill, Row, RowIndex, StyleId,
    WorkbookWriter, WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let path = arguments.next().ok_or("Missing output")?;
    let rows: u32 = arguments.next().as_deref().unwrap_or("4").parse()?;
    if rows == 0 || rows > 1_048_576 {
        return Err("Invalid row count".into());
    }
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    let mut identities = Vec::<StyleId>::new();
    identities.try_reserve_exact(5)?;
    for code in ["General", "0.00", "yyyy-mm-dd", "hh:mm:ss", "[h]:mm:ss"] {
        identities.push(writer.register_style(CellStyle {
            number_format: code.into(),
            font: Font {
                name: Some("Public temporal font".into()),
                size: Some(14.0),
                bold: Some(true),
                color: Some(Color {
                    kind: ColorKind::Argb(0xff112233),
                    tint: None,
                }),
                ..Default::default()
            },
            fill: Fill::Pattern(PatternFill {
                pattern: Some(FillPattern::Solid),
                foreground: Some(Color {
                    kind: ColorKind::Argb(0xff445566),
                    tint: None,
                }),
                ..Default::default()
            }),
            borders: {
                let mut border = Border::default();
                border.sides[0] = Some(BorderSide {
                    line: Some(BorderLine::Thin),
                    color: Some(Color {
                        kind: ColorKind::Argb(0xff778899),
                        tint: None,
                    }),
                });
                border
            },
            alignment: Alignment {
                indent: Some(2.5),
                wrap_text: Some(true),
                ..Default::default()
            },
            ..Default::default()
        })?);
    }
    let dates = [
        ExcelDateTime::from_ymd(2024, 1, 2)?,
        ExcelDateTime::from_ymd_hms_micro(2024, 1, 2, 3, 4, 5, 678900)?,
        ExcelDateTime::from_hms_micro(3, 4, 5, 678900)?,
        ExcelDateTime::from_duration_parts(2, 3, 678900)?,
    ];
    writer.start_sheet("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    row.cells.try_reserve_exact(5)?;
    for index in 0..rows {
        row.index = RowIndex::new(index)?;
        row.cells.clear();
        for (column, id) in identities.iter().enumerate() {
            row.cells.push(Cell {
                address: CellAddress::new(index, column as u32)?,
                value: CellValue::DateTime(Box::new(dates[index as usize % dates.len()])),
                style: *id,
            });
        }
        writer.write_row(&row)?;
    }
    writer.close_sheet()?;
    eprintln!("STYLE_BYTES {}", writer.style_memory_bytes());
    eprintln!("TEMP_BYTES {}", writer.stats().peak_temp_bytes);
    writer.finish(std::fs::File::create(path)?)?;
    println!("{}", rows * 5);
    Ok(())
}
