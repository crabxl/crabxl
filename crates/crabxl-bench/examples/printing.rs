//! Canonical printing creation, streamed source export and original-package editing.
use crabxl::{
    Cell, CellValue, PageBreak, PageMargins, PageOrder, PageOrientation, PaperDimension,
    PrintSettings, PrintedComments, PrintedErrors, Result, Row, RowIndex, SaveOptions, StyleId,
    WorkbookEditor, WorkbookReader, WorkbookWriter, WriteOptions,
};
use std::{fs::File, io::BufWriter};
fn settings() -> Result<PrintSettings> {
    let mut value = PrintSettings {
        auto_page_breaks: Some(false),
        fit_to_page: Some(true),
        margins: Some(PageMargins {
            left: -1.5,
            right: 0.25,
            top: 0.5,
            bottom: 0.75,
            header: 0.2,
            footer: 0.3,
        }),
        ..PrintSettings::default()
    };
    value.options.horizontal_centered = Some(true);
    value.options.vertical_centered = Some(false);
    value.options.headings = Some(true);
    value.options.grid_lines = Some(false);
    value.options.grid_lines_set = Some(true);
    value.setup.orientation = Some(PageOrientation::Landscape);
    value.setup.paper_size = Some(9);
    value.setup.scale = Some(85);
    value.setup.fit_to_height = Some(0);
    value.setup.fit_to_width = Some(1);
    value.setup.first_page_number = Some(7);
    value.setup.use_first_page_number = Some(true);
    value.setup.paper_height = Some(PaperDimension::parse("11.5in tail")?);
    value.setup.paper_width = Some(PaperDimension::parse("8.25in")?);
    value.setup.page_order = Some(PageOrder::OverThenDown);
    value.setup.use_printer_defaults = Some(false);
    value.setup.black_and_white = Some(true);
    value.setup.draft = Some(false);
    value.setup.cell_comments = Some(PrintedComments::AtEnd);
    value.setup.errors = Some(PrintedErrors::NotAvailable);
    value.setup.horizontal_dpi = Some(300);
    value.setup.vertical_dpi = Some(600);
    value.setup.copies = Some(2);
    value.row_breaks = vec![
        PageBreak {
            id: Some(10),
            minimum: Some(2),
            maximum: Some(7),
            manual: Some(false),
            pivot: Some(true),
        },
        PageBreak {
            id: Some(25),
            ..PageBreak::default()
        },
    ];
    value.row_breaks.push(PageBreak {
        id: None,
        minimum: None,
        maximum: None,
        manual: Some(false),
        pivot: Some(false),
    });
    value.column_breaks = vec![PageBreak {
        id: Some(3),
        minimum: Some(1),
        maximum: Some(1048575),
        manual: None,
        pivot: Some(false),
    }];
    Ok(value)
}
fn output(path: &str) -> Result<BufWriter<File>> {
    File::create(path).map(BufWriter::new).map_err(|error| {
        crabxl::Error::caused_by(
            crabxl::ErrorKind::Io,
            "Cannot create printing fixture",
            error,
        )
    })
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(crabxl::Error::new(
            crabxl::ErrorKind::InvalidData,
            "Usage: printing create|copy|edit SOURCE OUTPUT ROWS",
        ));
    }
    if args[1] == "edit" {
        let mut editor = WorkbookEditor::open(&args[2])?;
        editor.set_print_settings("Sheet", settings()?)?;
        println!("PRINT_BYTES={}", editor.patch_bytes());
        editor.save(output(&args[3])?, SaveOptions::default())?;
        println!("TEMP_BYTES=0");
        return Ok(());
    }
    if !matches!(args[1].as_str(), "create" | "copy") {
        return Err(crabxl::Error::new(
            crabxl::ErrorKind::InvalidData,
            "Unknown printing operation",
        ));
    }
    let mut reader = if args[1] == "copy" {
        Some(WorkbookReader::open(&args[2])?)
    } else {
        None
    };
    let value = match &mut reader {
        Some(reader) => reader.print_settings("Sheet")?,
        None => settings()?,
    };
    println!("PRINT_BYTES={}", value.memory_bytes());
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet_with_settings("Sheet", None, Some(&value))?;
    if let Some(reader) = &mut reader {
        let mut rows = reader.rows("Sheet")?;
        while let Some(row) = rows.next_row()? {
            writer.write_row(&row)?;
        }
    } else {
        let count: u32 = args[4].parse().map_err(|error| {
            crabxl::Error::caused_by(
                crabxl::ErrorKind::InvalidData,
                "Invalid printing row count",
                error,
            )
        })?;
        let mut row = Row::new(RowIndex::new(0)?);
        for index in 0..count {
            row.index = RowIndex::new(index)?;
            row.cells.clear();
            row.cells.push(Cell {
                address: crabxl::CellAddress::new(index, 0)?,
                value: CellValue::Integer(i64::from(index)),
                style: StyleId::new(0),
            });
            writer.write_row(&row)?;
        }
    }
    writer.close_sheet()?;
    println!("TEMP_BYTES={}", writer.stats().peak_temp_bytes);
    writer.finish(output(&args[3])?)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
