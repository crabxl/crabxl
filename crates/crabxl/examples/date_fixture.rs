//! Create literal microsecond date/time/duration values for public readback.
use crabxl::{
    Cell, CellAddress, CellValue, DateEpoch, ExcelDateTime, Row, RowIndex, StyleId, WorkbookWriter,
    WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: date_fixture <output> [mac]")?;
    let epoch = if std::env::args().nth(2).as_deref() == Some("mac") {
        DateEpoch::Mac1904
    } else {
        DateEpoch::Windows1900
    };
    let values = [
        ExcelDateTime::from_ymd_hms_micro(2024, 2, 29, 12, 3, 4, 123456)?,
        ExcelDateTime::from_hms_micro(2, 57, 46, 666570)?,
        ExcelDateTime::from_duration_parts(-1, 86399, 999999)?,
        ExcelDateTime::from_ymd_hms_micro(1899, 12, 31, 12, 0, 0, 123456)?,
    ];
    let mut writer = WorkbookWriter::new(WriteOptions {
        date_1904: epoch == DateEpoch::Mac1904,
        ..Default::default()
    })?;
    writer.start_sheet("Sheet")?;
    let cells = values
        .into_iter()
        .enumerate()
        .map(|(column, value)| {
            Ok(Cell {
                address: CellAddress::new(0, column as u32)?,
                value: CellValue::DateTime(Box::new(value)),
                style: StyleId::new(0),
            })
        })
        .collect::<Result<Vec<_>, crabxl::Error>>()?;
    writer.write_row(&Row {
        index: RowIndex::new(0)?,
        cells,
    })?;
    writer.finish(std::fs::File::create(path)?)?;
    Ok(())
}
