//! Streaming view creation, source read/export and repeatable original-package editing.
use crabxl::{
    Cell, CellValue, Pane, PanePosition, PaneState, Result, Row, RowIndex, SaveOptions, SheetView,
    SheetViews, StyleId, ViewMode, WorkbookEditor, WorkbookReader, WorkbookWriter, WriteOptions,
};
use std::{fs::File, io::BufWriter};
fn views() -> Result<SheetViews> {
    let mut result = SheetViews::default();
    let view = &mut result.views[0];
    view.freeze_at(Some("B3".parse()?))?;
    view.window_protection = Some(false);
    view.show_formulas = Some(true);
    view.show_grid_lines = Some(false);
    view.show_row_column_headers = Some(false);
    view.show_zeros = Some(false);
    view.right_to_left = Some(true);
    view.tab_selected = Some(true);
    view.show_ruler = Some(false);
    view.show_outline_symbols = Some(false);
    view.default_grid_color = Some(false);
    view.show_white_space = Some(false);
    view.zoom_to_fit = Some(true);
    view.mode = Some(ViewMode::PageLayout);
    view.top_left_cell = Some("B3".into());
    view.color_id = Some(12);
    view.zoom_scale = Some(145);
    view.zoom_scale_normal = Some(80);
    view.zoom_scale_sheet_layout = Some(90);
    view.zoom_scale_page_layout = Some(95);
    view.selections[2].active_cell_id = Some(2);
    view.selections[2].ranges = Some("A1 C3:D4".into());
    result.views.push(SheetView {
        workbook_view_id: 2,
        pane: Some(Box::new(Pane {
            x_split: Some(-1.5),
            y_split: Some(2.5),
            top_left_cell: Some("opaque".into()),
            active_pane: PanePosition::BottomRight,
            state: PaneState::FrozenSplit,
        })),
        ..SheetView::default()
    });
    Ok(result)
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(crabxl::Error::new(
            crabxl::ErrorKind::InvalidData,
            "Usage: worksheet_views create|copy|edit SOURCE OUTPUT ROWS",
        ));
    }
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    match args[1].as_str() {
        "create" => {
            let rows: u32 = args[4].parse().map_err(|error| {
                crabxl::Error::caused_by(crabxl::ErrorKind::InvalidData, "Invalid row count", error)
            })?;
            let settings = views()?;
            println!("VIEW_BYTES={}", settings.memory_bytes());
            writer.start_sheet_with_views("Sheet", &settings)?;
            for index in 0..rows {
                writer.write_row(&Row {
                    index: RowIndex::new(index)?,
                    cells: vec![Cell {
                        address: crabxl::CellAddress::new(index, 0)?,
                        value: CellValue::Integer(i64::from(index)),
                        style: StyleId::new(0),
                    }],
                })?;
            }
            writer.close_sheet()?;
            println!("TEMP_BYTES={}", writer.stats().peak_temp_bytes);
            writer.finish(BufWriter::new(File::create(&args[3]).map_err(|error| {
                crabxl::Error::caused_by(crabxl::ErrorKind::Io, "Cannot create view fixture", error)
            })?))?;
        }
        "copy" => {
            let mut reader = WorkbookReader::open(&args[2])?;
            let settings = reader.sheet_views("Sheet")?;
            println!("VIEW_BYTES={}", settings.memory_bytes());
            writer.start_sheet_with_views("Sheet", &settings)?;
            let mut rows = reader.rows("Sheet")?;
            while let Some(row) = rows.next_row()? {
                writer.write_row(&row)?;
            }
            writer.close_sheet()?;
            println!("TEMP_BYTES={}", writer.stats().peak_temp_bytes);
            writer.finish(BufWriter::new(File::create(&args[3]).map_err(|error| {
                crabxl::Error::caused_by(crabxl::ErrorKind::Io, "Cannot create view fixture", error)
            })?))?;
        }
        "edit" => {
            let mut editor = WorkbookEditor::open(&args[2])?;
            editor.set_sheet_views("Sheet", views()?)?;
            println!("VIEW_BYTES={}", editor.patch_bytes());
            editor.save(
                BufWriter::new(File::create(&args[3]).map_err(|error| {
                    crabxl::Error::caused_by(
                        crabxl::ErrorKind::Io,
                        "Cannot create view fixture",
                        error,
                    )
                })?),
                SaveOptions::default(),
            )?;
            println!("TEMP_BYTES=0");
        }
        _ => {
            return Err(crabxl::Error::new(
                crabxl::ErrorKind::InvalidData,
                "Unknown view operation",
            ));
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
