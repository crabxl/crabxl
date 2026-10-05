//! Generate bounded numeric or mixed scalar sheets for interoperability benchmarks.
use crabxl::{
    BorderLine, BorderSide, Cell, CellAddress, CellStyle, CellValue, DateEpoch, DateKind,
    ExactInteger, ExcelDateTime, Formula, HorizontalAlignment, Row, RowIndex, VerticalAlignment,
    WorkbookWriter, WriteOptions,
};
use std::{fs::File, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: write_demo <output.xlsx> <rows> [numeric|mixed|features|features1904] [temp directory]")?;
    let count = args.next().ok_or("Missing row count")?.parse::<u32>()?;
    let mode = args.next().unwrap_or_else(|| "numeric".into());
    if !matches!(
        mode.as_str(),
        "numeric" | "mixed" | "features" | "features1904"
    ) {
        return Err("Unknown workload".into());
    }
    if count == 0 || count > crabxl::MAX_ROWS {
        return Err("Row count must fit an XLSX worksheet".into());
    }
    let mut writer = WorkbookWriter::new(WriteOptions {
        temp_directory: args.next().filter(|value| value != "-").map(PathBuf::from),
        compression_level: args.next().map(|value| value.parse()).transpose()?,
        date_1904: mode == "features1904",
        ..WriteOptions::default()
    })?;
    let style = if mode.starts_with("features") {
        let mut style = CellStyle {
            number_format: "0.00".into(),
            fill: crabxl::Fill::Pattern(crabxl::PatternFill {
                pattern: Some(crabxl::FillPattern::Solid),
                foreground: Some(crabxl::Color {
                    kind: crabxl::ColorKind::Argb(0xFFFFE699),
                    tint: None,
                }),
                background: Some(crabxl::Color {
                    kind: crabxl::ColorKind::Indexed(64),
                    tint: None,
                }),
            }),
            alignment: crabxl::Alignment {
                horizontal: Some(HorizontalAlignment::Center),
                vertical: Some(VerticalAlignment::Top),
                wrap_text: Some(true),
                rotation: Some(30),
                ..Default::default()
            },
            protection: crabxl::Protection {
                locked: Some(false),
                hidden: Some(true),
            },
            ..CellStyle::default()
        };
        style.font.bold = Some(true);
        style.font.italic = Some(true);
        style.font.underline = Some(crabxl::Underline::Single);
        style.font.color = Some(crabxl::Color {
            kind: crabxl::ColorKind::Argb(0xFF123456),
            tint: None,
        });
        style.borders.sides[0] = Some(BorderSide {
            line: Some(BorderLine::Thin),
            color: Some(crabxl::Color {
                kind: crabxl::ColorKind::Argb(0xFFABCDEF),
                tint: None,
            }),
        });
        writer.register_style(style)?
    } else {
        crabxl::StyleId::new(0)
    };
    writer.start_sheet("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    for index in 0..count {
        row.index = RowIndex::new(index)?;
        row.cells.clear();
        let values: [CellValue; 10] = if mode == "numeric" {
            std::array::from_fn(|column| CellValue::Integer(i64::from(index) * 10 + column as i64))
        } else if mode.starts_with("features") {
            [
                CellValue::DateTime(Box::new(ExcelDateTime::from_ymd_hms_milli(
                    2024, 2, 29, 12, 34, 56, 789,
                )?)),
                CellValue::DateTime(Box::new(ExcelDateTime::from_serial(
                    6.5 / 24.0,
                    DateEpoch::Windows1900,
                    DateKind::Time,
                )?)),
                CellValue::DateTime(Box::new(ExcelDateTime::from_serial(
                    1.5,
                    DateEpoch::Windows1900,
                    DateKind::Duration,
                )?)),
                CellValue::Formula(Box::new(Formula::new("=1+1", None)?)),
                CellValue::Formula(Box::new(Formula::new("=0", Some(CellValue::Integer(0)))?)),
                CellValue::Formula(Box::new(Formula::new(
                    "=FALSE()",
                    Some(CellValue::Boolean(false)),
                )?)),
                CellValue::Formula(Box::new(Formula::new(
                    "=\" cached \"",
                    Some(CellValue::text(" cached ")),
                )?)),
                CellValue::Formula(Box::new(Formula::new(
                    "=1/0",
                    Some(CellValue::error("#DIV/0!")),
                )?)),
                CellValue::Number(1.25),
                CellValue::text(" styled "),
            ]
        } else {
            [
                CellValue::Integer(9007199254740993),
                CellValue::BigInteger(Box::new(ExactInteger::parse(
                    "999999999999999999999999999999",
                )?)),
                CellValue::Number(1.25),
                CellValue::Boolean(index % 2 != 0),
                CellValue::error("#DIV/0!"),
                CellValue::text(" repeated "),
                CellValue::text(format!("row{index}").into_boxed_str()),
                CellValue::text(""),
                CellValue::Empty,
                CellValue::Integer(i64::from(index)),
            ]
        };
        for (column, value) in values.into_iter().enumerate() {
            row.cells.push(Cell {
                address: CellAddress::new(index, column as u32)?,
                value,
                style: if column >= 8 {
                    style
                } else {
                    crabxl::StyleId::new(0)
                },
            });
        }
        writer.write_row(&row)?;
    }
    writer.close_sheet()?;
    let stats = writer.stats();
    writer.finish(File::create(path)?)?;
    println!("{} {} {}", stats.rows, stats.cells, stats.peak_temp_bytes);
    Ok(())
}
