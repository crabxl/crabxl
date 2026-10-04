use calamine::{Data, Reader, Xlsx, open_workbook};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: calamine-baseline <workbook.xlsx>")?;
    let mut workbook: Xlsx<_> = open_workbook(path)?;
    let range = workbook.worksheet_range("Sheet")?;
    let mut sum = 0f64;
    let mut count = 0usize;
    for row in range.rows() {
        for value in row {
            match value {
                Data::Float(v) => {
                    sum += v;
                    count += 1;
                }
                Data::Int(v) => {
                    sum += *v as f64;
                    count += 1;
                }
                _ => return Err("Unexpected cell type".into()),
            }
        }
    }
    println!("{count} {sum:.0}");
    Ok(())
}
