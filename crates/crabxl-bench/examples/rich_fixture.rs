//! Write a small rich interoperability fixture with font and phonetic metadata.
use crabxl::{Cell, CellAddress, CellValue, Row, RowIndex, StyleId, WorkbookWriter, WriteOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: rich_fixture <output.xlsx>")?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sheet")?;
    writer.write_row(&Row {
        index: RowIndex::new(0)?,
        cells: vec![Cell {
            address: CellAddress::new(0, 0)?,
            value: rich_value(),
            style: StyleId::new(0),
        }],
    })?;
    writer.finish(std::fs::File::create(path)?)?;
    Ok(())
}
fn rich_value() -> CellValue {
    use crabxl::{
        Color, ColorKind, FontScheme, PhoneticProperties, PhoneticRun, RichText, RichTextRun,
        RunFont, TextVerticalAlignment, Underline,
    };
    CellValue::RichText(Box::new(RichText {
        runs: vec![
            RichTextRun {
                text: " <&> _x005F_x0041_ \r\n🦀 ".into(),
                font: Some(Box::new(RunFont {
                    name: Some("Quoted \" &\t\n\r".into()),
                    size: Some(12.5),
                    bold: Some(true),
                    italic: Some(false),
                    strike: Some(false),
                    outline: Some(true),
                    shadow: Some(true),
                    condense: Some(false),
                    extend: Some(true),
                    underline: Some(Underline::DoubleAccounting),
                    vertical: Some(TextVerticalAlignment::Superscript),
                    charset: Some(128.into()),
                    family: Some(3.0),
                    scheme: Some(FontScheme::Minor),
                    color: Some(Color {
                        kind: ColorKind::Argb(0x80445566),
                        tint: Some(-0.25),
                    }),
                })),
            },
            RichTextRun {
                text: "tail".into(),
                font: None,
            },
            RichTextRun {
                text: "".into(),
                font: Some(Box::default()),
            },
        ],
        phonetic_runs: vec![PhoneticRun {
            start: 0,
            end: 2,
            text: " pronunciation ".into(),
        }],
        phonetic_properties: Some(Box::new(PhoneticProperties {
            font_id: 0,
            kind: Some("Hiragana".into()),
            alignment: Some("center".into()),
        })),
    }))
}
