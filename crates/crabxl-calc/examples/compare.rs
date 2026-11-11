//! Calculate one workbook and compare each formula result with the value
//! Excel cached in the file.
//!
//! Run with `cargo run -p crabxl-calc --release --example compare FILE`.
//! Prints `M` lines for mismatches and one final `S` summary line:
//! formulas, cached, matched, skipped.
use crabxl_calc::Calculation;
use crabxl_core::{CellAddress, CellValue, DateEpoch, ReadOptions};
use crabxl_xlsx::{SheetKind, WorkbookReader};
use std::collections::HashMap;

fn same(cached: &CellValue, got: &CellValue) -> bool {
    let number = |value: &CellValue| match value {
        CellValue::Number(number) => Some(*number),
        CellValue::Integer(integer) => Some(*integer as f64),
        CellValue::DateTime(date) => Some(date.serial()),
        _ => None,
    };
    match (number(cached), number(got)) {
        (Some(a), Some(b)) => a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0),
        _ => cached == got,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Expected a workbook path")?;
    let mut reader = WorkbookReader::open(&path)?;
    let epoch = if reader.date_1904() {
        DateEpoch::Mac1904
    } else {
        DateEpoch::Windows1900
    };
    let sheets: Vec<(String, bool)> = reader
        .sheets()
        .iter()
        .map(|sheet| {
            (
                sheet.name().to_owned(),
                sheet.kind() == SheetKind::Worksheet,
            )
        })
        .collect();

    let mut calculation = Calculation::new(epoch, crabxl_xlsx::max_threads());
    let mut cached: HashMap<(usize, CellAddress), CellValue> = HashMap::new();
    let mut expressions: HashMap<(usize, CellAddress), String> = HashMap::new();
    let data_only = ReadOptions {
        data_only: true,
        ..ReadOptions::default()
    };
    for (name, worksheet) in &sheets {
        let position = calculation.add_sheet(name)?;
        if !worksheet {
            continue;
        }
        let mut rows = reader.rows(name)?;
        while let Some(row) = rows.next_row()? {
            for cell in &row.cells {
                calculation.add_cell(position, cell)?;
                if let CellValue::Formula(formula) = cell.value.unannotated() {
                    expressions.insert((position, cell.address), formula.expression().into());
                }
            }
        }
        drop(rows);
        let mut rows = reader.rows_with_options(name, data_only.clone())?;
        while let Some(row) = rows.next_row()? {
            for cell in row.cells {
                cached.insert((position, cell.address), cell.value);
            }
        }
        drop(rows);
        for table in &reader.sheet_features(name)?.tables {
            calculation.add_table(position, table)?;
        }
    }
    for name in &reader.workbook_features()?.defined_names {
        calculation.add_name(name);
    }

    let (mut formulas, mut with_cache, mut matched, mut skipped) = (0usize, 0, 0, 0);
    calculation.run(|sheet, address, got| {
        formulas += 1;
        skipped += usize::from(got.is_none());
        let Some(expected) = cached
            .get(&(sheet, address))
            .filter(|value| **value != CellValue::Empty)
        else {
            return Ok(());
        };
        with_cache += 1;
        match &got {
            Some(got) if same(expected, got) => matched += 1,
            _ => println!(
                "M\t{}!{}\t{}\t{:?}\t{:?}",
                sheets[sheet].0,
                address,
                expressions
                    .get(&(sheet, address))
                    .map_or("", String::as_str),
                expected,
                got
            ),
        }
        Ok(())
    })?;
    println!("S\t{formulas}\t{with_cache}\t{matched}\t{skipped}");
    Ok(())
}
