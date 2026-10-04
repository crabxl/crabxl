use calamine::{Data, Reader, Xlsx, open_workbook};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: shared_text <xlsx> <unique-count>".into());
    }
    let unique: u64 = args[2].parse()?;
    if unique == 0 {
        return Err("Unique count must be positive".into());
    }
    let mut book: Xlsx<_> = open_workbook(&args[1])?;
    let range = book.worksheet_range("Sheet")?;
    let mut count = 0u64;
    let mut bytes = 0u64;
    let suffix = "x".repeat(96);
    for row in range.rows() {
        for cell in row {
            let Data::String(text) = cell else {
                return Err("Expected string".into());
            };
            if text != &format!("item-{:08}-{suffix}", count % unique) {
                return Err("Shared-string value/order mismatch".into());
            }
            bytes += text.len() as u64;
            count += 1;
        }
    }
    println!("{count} {bytes}");
    Ok(())
}
