//! Create, stream, edit and re-read a small workbook.
//!
//! Run with `cargo run -p crabxl --example quickstart [output directory]`.
//! The files are written to the system temporary directory by default.
use crabxl::read::{Reader, RowOptions};
use crabxl::style::{Font, Style};
use crabxl::write::{WriteOptions, Writer};
use crabxl::{Formula, Workbook};
use std::path::PathBuf;

fn main() -> crabxl::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let created = directory.join("crabxl-quickstart.xlsx");
    let edited = directory.join("crabxl-quickstart-edited.xlsx");

    // 1. Write a workbook row by row.
    let header = Style::new().with_font(Font::new().with_bold(true));
    let mut writer = Writer::create(&created, &WriteOptions::new())?;
    let mut sheet = writer.add_worksheet("Sales")?;
    sheet.freeze_panes("A2")?;
    sheet.append_row_styled(("Region", "Amount"), &header)?;
    for (region, amount) in [("North", 120), ("South", 95), ("West", 143)] {
        sheet.append_row((region, amount))?;
    }
    sheet.append_row(("Total", Formula::new("SUM(B2:B4)")?))?;
    writer.finish()?;

    // 2. Stream the rows back.
    let mut reader = Reader::open(&created)?;
    for row in reader.rows("Sales", &RowOptions::new().with_skip_rows(1))? {
        let row = row?;
        println!("row {}: {:?}", row.index() + 1, row.values());
    }

    // 3. Edit the file: insert a row, and the total formula follows.
    let mut book = Workbook::open(&created)?;
    let mut sales = book.worksheet("Sales")?;
    sales.insert_rows("3")?;
    sales.set_values("A3", [("East", 77)])?;
    println!("total formula: {:?}", sales.formula("B6")?);
    book.save(&edited)?;
    println!("wrote {} and {}", created.display(), edited.display());
    Ok(())
}
