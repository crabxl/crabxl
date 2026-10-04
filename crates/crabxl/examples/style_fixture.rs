//! Create a complete style-component fixture for public API interoperability.
use crabxl::{Cell, CellAddress, CellValue, Row, RowIndex, WorkbookWriter, WriteOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: style_fixture <output.xlsx>")?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    let style = writer.register_style(complete_style())?;
    writer.start_sheet("Sheet")?;
    writer.write_row(&Row {
        index: RowIndex::new(0)?,
        cells: vec![Cell {
            address: CellAddress::new(0, 0)?,
            value: CellValue::Number(1.5),
            style,
        }],
    })?;
    writer.finish(std::fs::File::create(path)?)?;
    Ok(())
}
fn complete_style() -> crabxl::CellStyle {
    use crabxl::*;
    let mut style = CellStyle {
        number_format: "0.00".into(),
        font: Font {
            name: Some("A".repeat(80).into()),
            size: Some(12.5),
            bold: Some(false),
            italic: Some(true),
            strike: Some(true),
            outline: Some(false),
            shadow: Some(true),
            condense: Some(false),
            extend: Some(true),
            underline: Some(Underline::DoubleAccounting),
            vertical: Some(TextVerticalAlignment::Subscript),
            charset: Some(128),
            family: Some(3.0),
            scheme: Some(FontScheme::Major),
            color: Some(Color {
                kind: ColorKind::Argb(0x80445566),
                tint: Some(-0.25),
            }),
        },
        fill: Fill::Gradient(GradientFill {
            kind: Some(GradientKind::Path),
            degree: Some(35.0),
            edges: [Some(0.1), Some(0.2), Some(0.3), Some(0.4)],
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: Color {
                        kind: ColorKind::Argb(0x80AABBCC),
                        tint: None,
                    },
                },
                GradientStop {
                    position: 1.0,
                    color: Color {
                        kind: ColorKind::Indexed(64),
                        tint: Some(0.0),
                    },
                },
            ],
        }),
        borders: Border {
            sides: [None; 9],
            diagonal_up: Some(true),
            diagonal_down: Some(false),
            outline: Some(false),
        },
        alignment: Alignment {
            horizontal: Some(HorizontalAlignment::Distributed),
            vertical: Some(VerticalAlignment::Justify),
            rotation: Some(255),
            wrap_text: Some(true),
            shrink_to_fit: Some(true),
            indent: Some(2.5),
            relative_indent: Some(-1.5),
            reading_order: Some(2.0),
            justify_last_line: Some(true),
            merge_cell: None,
        },
        protection: Protection {
            locked: Some(false),
            hidden: Some(true),
        },
    };
    for (i, line) in [
        BorderLine::Thin,
        BorderLine::Medium,
        BorderLine::Thick,
        BorderLine::Dashed,
        BorderLine::SlantDashDot,
        BorderLine::MediumDashDot,
        BorderLine::MediumDashDotDot,
        BorderLine::Hair,
        BorderLine::None,
    ]
    .into_iter()
    .enumerate()
    {
        style.borders.sides[i] = Some(BorderSide {
            line: Some(line),
            color: Some(Color {
                kind: ColorKind::Auto(i % 2 == 0),
                tint: Some(0.25),
            }),
        });
    }
    style
}
