use calamine::{Data, Reader, Xlsx, open_workbook};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Usage: styled_read <file>")?;
    let mut book: Xlsx<_> = open_workbook(path)?;
    let range = book.worksheet_range("Sheet")?;
    let mut count = 0u64;
    for row in range.rows() {
        let index = count / 10;
        if row.len() != 10 { return Err("Wrong row width".into()); }
        let mut day = index % 365 + 1;
        let mut month = 1u8;
        for days in [31,29,31,30,31,30,31,31,30,31,30,31] {
            if day <= days { break; }
            day -= days;
            month += 1;
        }
        for (column, value) in row.iter().enumerate() {
            let matches = match (column, value) {
                (0 | 9, Data::Float(value)) => *value == 1.25,
                (1 | 7, Data::DateTime(value)) => !value.is_duration() && value.as_f64() == 45292.25 + (index % 365) as f64 && value.to_ymd_hms_milli() == (2024, month, day as u8, 6, 0, 0, 0),
                (2, Data::DateTime(value)) => !value.is_duration() && value.as_f64() == 0.5 && value.to_ymd_hms_milli().3 == 12,
                (3, Data::DateTime(value)) => value.is_duration() && value.as_f64() == (index % 101) as f64 / 4.0,
                (4, Data::Bool(value)) => *value == (index % 2 != 0),
                (5, Data::String(value)) => value == &format!("styled-{index:08}"),
                (6, Data::Error(value)) => value.to_string() == "#DIV/0!",
                (8, Data::Float(value)) => *value == index as f64,
                (8, Data::Int(value)) => *value == index as i64,
                _ => false,
            };
            if !matches { return Err(format!("Wrong value at {index}:{column}: {value:?}").into()); }
            count += 1;
        }
    }
    println!("{count}");
    Ok(())
}
