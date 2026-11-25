//! End-to-end tests of the public API.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl::features::{
    Comparison, ConditionalFormat, DataValidation, DefinedName, Hyperlink, Image, Note, Table,
};
use crabxl::read::{Reader, RowOptions};
use crabxl::style::{Border, BorderLine, BorderSide, Color, Fill, Font, PartialStyle, Style};
use crabxl::write::{WriteOptions, Writer};
use crabxl::{
    CellRef, DataLossPolicy, ErrorKind, ErrorValue, Formula, Number, OpenOptions, SaveOptions,
    Temporal, TemporalKind, Value, Visibility, Workbook,
};
use std::io::Cursor;

fn reopen(book: &mut Workbook) -> Workbook {
    let bytes = book
        .write_to(Cursor::new(Vec::new()), &SaveOptions::new())
        .unwrap();
    Workbook::from_reader(bytes, &OpenOptions::new()).unwrap()
}

fn png() -> Vec<u8> {
    // A valid 1x1 PNG.
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0,
        0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 10, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0D, 0x0A, 0x2D, 0xB4, 0, 0, 0, 0, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

#[test]
fn new_workbooks_start_empty_and_need_a_sheet() {
    let mut book = Workbook::new().unwrap();
    assert!(book.sheet_names().is_empty());
    let error = book
        .write_to(Cursor::new(Vec::new()), &SaveOptions::new())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::NoVisibleSheet);
    book.add_worksheet("Data").unwrap();
    assert_eq!(book.sheet_names(), ["Data"]);
    assert_eq!(
        book.add_worksheet("data").unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        book.worksheet("Missing").err().unwrap().kind(),
        ErrorKind::SheetNotFound
    );
    let reopened = reopen(&mut book);
    assert_eq!(reopened.sheet_names(), ["Data"]);
}

#[test]
fn values_formulas_and_types_round_trip() {
    let mut book = Workbook::new().unwrap();
    let mut sheet = book.add_worksheet("Values").unwrap();
    let date = Temporal::date(2026, 11, 25).unwrap();
    let exact = Number::exact("3.14159265358979323846").unwrap();
    sheet
        .set_values(
            "A1",
            [vec![
                Value::from(42),
                Value::from(0.1),
                Value::from("=not a formula"),
                Value::from(true),
                Value::Temporal(date),
                Value::Number(exact.clone()),
                Value::Error(ErrorValue::Div0),
                Value::Blank,
            ]],
        )
        .unwrap();
    sheet.set_formula("I1", "=A1*2").unwrap();
    assert_eq!(sheet.value("Z99").unwrap(), Value::Blank);
    assert_eq!(sheet.used_range().unwrap().unwrap().to_string(), "A1:I1");

    let mut book = reopen(&mut book);
    let mut sheet = book.worksheet("values").unwrap();
    assert_eq!(sheet.value("A1").unwrap(), Value::from(42));
    assert_eq!(sheet.value("B1").unwrap().as_f64(), Some(0.1));
    assert_eq!(sheet.value("C1").unwrap().as_str(), Some("=not a formula"));
    assert_eq!(sheet.formula("C1").unwrap(), None);
    assert_eq!(sheet.value("D1").unwrap(), Value::Bool(true));
    let Value::Temporal(read) = sheet.value("E1").unwrap() else {
        panic!()
    };
    assert_eq!(read.kind(), TemporalKind::Date);
    assert_eq!(read.to_iso8601().unwrap(), "2026-11-25");
    let Value::Number(read) = sheet.value("F1").unwrap() else {
        panic!()
    };
    assert_eq!(read.to_exact_string(), "3.14159265358979323846");
    assert_eq!(sheet.value("G1").unwrap(), Value::Error(ErrorValue::Div0));
    assert_eq!(
        sheet.formula("I1").unwrap(),
        Some(Formula::new("A1*2").unwrap())
    );
    let typed: Vec<(i64, f64, String)> = sheet.rows_as("A1:C1").unwrap();
    assert_eq!(typed, [(42, 0.1, "=not a formula".to_owned())]);
    let error = sheet.rows_as::<(f64,)>("C1").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Conversion);
    assert_eq!(error.sheet(), Some("Values"));
    assert!(sheet.set_value("A2", f64::NAN).is_err());
    assert!(sheet.set_value("A3", "x".repeat(32_768)).is_err());
    assert_eq!(
        sheet.set_values("A1:B2", [(1, 2)]).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn exact_number_mode_returns_every_decimal_as_written() {
    let mut book = Workbook::new().unwrap();
    book.add_worksheet("S")
        .unwrap()
        .set_value("A1", 0.5)
        .unwrap();
    let bytes = book
        .write_to(Cursor::new(Vec::new()), &SaveOptions::new())
        .unwrap();
    let mut exact =
        Workbook::from_reader(bytes, &OpenOptions::new().with_exact_numbers(true)).unwrap();
    let Value::Number(number) = exact.worksheet("S").unwrap().value("A1").unwrap() else {
        panic!()
    };
    assert_eq!(number.to_exact_string(), "0.5");
}

#[test]
fn styles_layout_and_merges() {
    let mut book = Workbook::new().unwrap();
    let mut sheet = book.add_worksheet("Report").unwrap();
    sheet.append_row(("Region", "Amount")).unwrap();
    sheet.append_row(("North", 12.5)).unwrap();
    let header = Style::new()
        .with_font(
            Font::new()
                .with_bold(true)
                .with_color(Color::rgb(255, 255, 255)),
        )
        .with_fill(Fill::solid(Color::hex("1F4E79").unwrap()))
        .with_border(Border::all(BorderSide::new(
            BorderLine::Thin,
            Color::rgb(0, 0, 0),
        )));
    sheet.set_style("A1:B1", &header).unwrap();
    sheet
        .map_style("B2", |style| style.with_number_format("#,##0.00"))
        .unwrap();
    sheet.set_column_width("A:B", 18.0).unwrap();
    sheet.set_row_height("1", 24.0).unwrap();
    sheet.freeze_panes("A2").unwrap();
    sheet.merge("D1:E2").unwrap();

    let mut book = reopen(&mut book);
    let mut sheet = book.worksheet("Report").unwrap();
    let style = sheet.style("A1").unwrap();
    assert_eq!(style.font().bold, Some(true));
    assert_eq!(style.font().name.as_deref(), Some("Calibri"));
    assert_eq!(style.fill(), &Fill::solid(Color::hex("1F4E79").unwrap()));
    assert_eq!(sheet.style("B2").unwrap().number_format(), "#,##0.00");
    assert_eq!(sheet.column_width("B").unwrap(), Some(18.0));
    assert_eq!(sheet.row_height("1").unwrap(), Some(24.0));
    assert_eq!(sheet.frozen_panes().unwrap(), Some("A2".parse().unwrap()));
    assert_eq!(sheet.merged_ranges().unwrap(), ["D1:E2".parse().unwrap()]);
    sheet.unmerge("D1:E2").unwrap();
    assert!(sheet.unmerge("D1:E2").is_err());
}

#[test]
fn objects_round_trip() {
    let mut book = Workbook::new().unwrap();
    let mut sheet = book.add_worksheet("Sales").unwrap();
    sheet.append_row(("Region", "Amount")).unwrap();
    sheet.append_rows([("North", 10), ("South", 20)]).unwrap();
    sheet
        .add_table(Table::new("SalesTable", "A1:B3").unwrap())
        .unwrap();
    assert!(
        sheet
            .add_table(Table::new("salestable", "D1:E3").unwrap())
            .is_err()
    );
    sheet
        .add_data_validation(
            DataValidation::list(["North", "South"])
                .unwrap()
                .with_range("A2:A3")
                .unwrap(),
        )
        .unwrap();
    let red = PartialStyle::new().with_fill(Fill::solid(Color::rgb(255, 199, 206)));
    sheet
        .add_conditional_format(
            ConditionalFormat::cell("B2:B3", Comparison::GreaterThan, 15, None, red).unwrap(),
        )
        .unwrap();
    sheet
        .set_note("B2", Note::new("Preliminary", "Finance"))
        .unwrap();
    sheet
        .set_hyperlink("A2", Hyperlink::url("https://example.com/north").unwrap())
        .unwrap();
    sheet
        .set_hyperlink("A3", Hyperlink::location("Sales!B3").unwrap())
        .unwrap();
    sheet
        .add_image(&Image::from_bytes(png()).unwrap(), "D5")
        .unwrap();
    sheet.protect(Some("secret")).unwrap();
    book.add_defined_name(DefinedName::new("Total", "Sales!$B$2:$B$3").unwrap())
        .unwrap();
    let mut properties = book.properties().unwrap();
    properties.title = Some("Q3".into());
    book.set_properties(properties).unwrap();

    let mut book = reopen(&mut book);
    assert_eq!(book.properties().unwrap().title.as_deref(), Some("Q3"));
    let names = book.defined_names().unwrap();
    assert!(
        names
            .iter()
            .any(|name| name.name() == "Total" && name.formula() == "Sales!$B$2:$B$3")
    );
    let mut sheet = book.worksheet("Sales").unwrap();
    let tables = sheet.tables().unwrap();
    assert_eq!(
        (tables[0].name(), tables[0].range().to_string()),
        ("SalesTable", "A1:B3".into())
    );
    assert_eq!(
        sheet.data_validations().unwrap()[0].formula1(),
        Some("\"North,South\"")
    );
    let formats = sheet.conditional_formats().unwrap();
    assert_eq!(formats[0].formulas().collect::<Vec<_>>(), ["15"]);
    assert_eq!(
        sheet.note("B2").unwrap(),
        Some(Note::new("Preliminary", "Finance"))
    );
    assert_eq!(
        sheet.hyperlink("A2").unwrap(),
        Some(Hyperlink::url("https://example.com/north").unwrap())
    );
    assert_eq!(
        sheet.hyperlink("A3").unwrap(),
        Some(Hyperlink::location("Sales!B3").unwrap())
    );
    assert_eq!(sheet.image_count().unwrap(), 1);
    assert!(sheet.is_protected().unwrap());
    sheet.remove_table("SalesTable").unwrap();
    assert_eq!(
        sheet.remove_table("SalesTable").unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert!(sheet.remove_note("B2").unwrap());
}

#[test]
fn structural_edits_update_references_across_sheets() {
    let mut book = Workbook::new().unwrap();
    let mut data = book.add_worksheet("Data").unwrap();
    data.append_rows([[1], [2], [3]]).unwrap();
    data.set_formula("A4", "SUM(A1:A3)").unwrap();
    let mut summary = book.add_worksheet("Summary").unwrap();
    summary.set_formula("A1", "Data!A4*2").unwrap();
    book.add_defined_name(DefinedName::new("Total", "Data!$A$4").unwrap())
        .unwrap();

    book.worksheet("Data").unwrap().insert_rows("2:3").unwrap();
    assert_eq!(
        book.worksheet("Data")
            .unwrap()
            .formula("A6")
            .unwrap()
            .unwrap()
            .text(),
        "SUM(A1:A5)"
    );
    assert_eq!(
        book.worksheet("Summary")
            .unwrap()
            .formula("A1")
            .unwrap()
            .unwrap()
            .text(),
        "Data!A6*2"
    );
    assert_eq!(book.defined_names().unwrap()[0].formula(), "Data!$A$6");

    book.worksheet("Data").unwrap().delete_columns("A").unwrap();
    assert_eq!(
        book.worksheet("Summary")
            .unwrap()
            .formula("A1")
            .unwrap()
            .unwrap()
            .text(),
        "Data!#REF!*2"
    );
}

#[test]
fn sheets_can_be_copied_moved_hidden_and_removed() {
    let mut book = Workbook::new().unwrap();
    book.add_worksheet("One")
        .unwrap()
        .set_value("A1", 1)
        .unwrap();
    book.add_worksheet("Two").unwrap();
    let mut copy = book.copy_worksheet("One", "Copy").unwrap();
    assert_eq!(copy.value("A1").unwrap(), Value::from(1));
    book.move_sheet("Copy", 0).unwrap();
    assert_eq!(book.sheet_names(), ["Copy", "One", "Two"]);
    book.set_visibility("Two", Visibility::Hidden).unwrap();
    book.set_active_sheet("One").unwrap();
    assert_eq!(book.active_sheet().as_deref(), Some("One"));
    book.remove_sheet("Copy").unwrap();
    let mut book = reopen(&mut book);
    assert_eq!(book.sheet_names(), ["One", "Two"]);
    assert_eq!(book.visibility("Two").unwrap(), Visibility::Hidden);
    assert_eq!(book.worksheet_at(0).unwrap().name(), "One");
}

#[test]
fn saving_over_the_source_path_is_atomic_and_safe() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("book.xlsx");
    let mut book = Workbook::new().unwrap();
    book.add_worksheet("S")
        .unwrap()
        .set_value("A1", "first")
        .unwrap();
    book.save(&path).unwrap();
    let mut book = Workbook::open(&path).unwrap();
    book.worksheet("S")
        .unwrap()
        .set_value("A2", "second")
        .unwrap();
    book.save(&path).unwrap();
    let mut again = Workbook::open(&path).unwrap();
    let mut sheet = again.worksheet("S").unwrap();
    assert_eq!(sheet.value("A1").unwrap().as_str(), Some("first"));
    assert_eq!(sheet.value("A2").unwrap().as_str(), Some("second"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    let saves = |book: &mut Workbook| {
        book.write_to(Cursor::new(Vec::new()), &SaveOptions::new())
            .unwrap()
            .into_inner()
    };
    assert_eq!(
        saves(&mut again),
        saves(&mut again),
        "Saves are byte-stable"
    );
}

#[test]
fn reader_streams_rows_with_projection() {
    let mut book = Workbook::new().unwrap();
    let mut sheet = book.add_worksheet("Events").unwrap();
    sheet.append_row(("id", "kind", "amount", "note")).unwrap();
    for id in 1..=5_i64 {
        sheet
            .append_row((
                id,
                "sale",
                id as f64 * 1.5,
                Formula::new(format!("C{}*2", id + 1)).unwrap(),
            ))
            .unwrap();
    }
    let bytes = book
        .write_to(Cursor::new(Vec::new()), &SaveOptions::new())
        .unwrap();
    let mut reader = Reader::from_reader(bytes, &OpenOptions::new()).unwrap();
    assert_eq!(reader.sheet_names(), ["Events"]);
    let options = RowOptions::new()
        .with_columns("A:C")
        .unwrap()
        .with_skip_rows(1)
        .with_max_rows(3);
    let rows: Vec<(i64, String, f64)> = reader
        .rows("events", &options)
        .unwrap()
        .map(|row| row.unwrap().parse().unwrap())
        .collect();
    assert_eq!(
        rows,
        [
            (1, "sale".into(), 1.5),
            (2, "sale".into(), 3.0),
            (3, "sale".into(), 4.5)
        ]
    );
    let mut all = reader.rows("Events", &RowOptions::new()).unwrap();
    let mut row = crabxl::read::Row::new();
    all.read_into(&mut row).unwrap();
    all.read_into(&mut row).unwrap();
    assert_eq!(row.index(), 1);
    assert_eq!(row.formula(3).unwrap().text(), "C2*2");
    drop(all);
    assert_eq!(
        reader
            .rows("Missing", &RowOptions::new())
            .err()
            .unwrap()
            .kind(),
        ErrorKind::SheetNotFound
    );
}

#[test]
fn writer_streams_sheets_with_layout_and_rules() {
    let header = Style::new().with_font(Font::new().with_bold(true));
    let mut writer = Writer::new(&WriteOptions::new().with_compression(1)).unwrap();
    let mut events = writer.add_worksheet("Events").unwrap();
    events.set_column_width("A:C", 16.0).unwrap();
    events.freeze_panes("A2").unwrap();
    events
        .append_row_styled(("id", "kind", "amount"), &header)
        .unwrap();
    for id in 0..100_i64 {
        events.append_row((id, "sale", 9.5)).unwrap();
    }
    assert_eq!(
        events.freeze_panes("B2").unwrap_err().kind(),
        ErrorKind::InvalidState
    );
    events.merge("E1:F1").unwrap();
    events
        .add_data_validation(
            DataValidation::whole_number(Comparison::Between, 0, Some(1000))
                .unwrap()
                .with_range("A2:A101")
                .unwrap(),
        )
        .unwrap();
    let mut other = writer.add_worksheet("Other").unwrap();
    other.append_row(["x"]).unwrap();
    writer
        .worksheet("Events")
        .unwrap()
        .append_row((100, "late", 1.0))
        .unwrap();
    let bytes = writer.finish_into(Cursor::new(Vec::new())).unwrap();

    let mut book = Workbook::from_reader(bytes, &OpenOptions::new()).unwrap();
    let mut sheet = book.worksheet("Events").unwrap();
    // The merge anchor E1 is a present cell.
    assert_eq!(sheet.used_range().unwrap().unwrap().to_string(), "A1:E102");
    assert_eq!(sheet.style("A1").unwrap().font().bold, Some(true));
    assert_eq!(
        sheet.frozen_panes().unwrap(),
        Some(CellRef::new(1, 0).unwrap())
    );
    assert_eq!(sheet.merged_ranges().unwrap().len(), 1);
    assert_eq!(sheet.data_validations().unwrap().len(), 1);
    assert_eq!(sheet.value("B102").unwrap().as_str(), Some("late"));
}

#[test]
fn saves_fail_rather_than_drop_unmodeled_content() {
    use std::io::{Read, Write};
    // A sheet protection element with an attribute that CrabXL does not model.
    let mut book = Workbook::new().unwrap();
    book.add_worksheet("S").unwrap().protect(None).unwrap();
    let bytes = book
        .write_to(Cursor::new(Vec::new()), &SaveOptions::new())
        .unwrap()
        .into_inner();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let name = file.name().unwrap().to_string();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        if name.starts_with("xl/worksheets/") && name.ends_with(".xml") {
            data = String::from_utf8(data)
                .unwrap()
                .replace("<sheetProtection ", "<sheetProtection futureFlag=\"1\" ")
                .into_bytes();
        }
        output
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&data).unwrap();
    }
    let source = output.finish().unwrap().into_inner();

    let mut strict =
        Workbook::from_reader(Cursor::new(source.clone()), &OpenOptions::new()).unwrap();
    strict.worksheet("S").unwrap().unprotect().unwrap();
    let error = strict
        .write_to(Cursor::new(Vec::new()), &SaveOptions::new())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);

    let mut allowing = Workbook::from_reader(
        Cursor::new(source),
        &OpenOptions::new().with_data_loss(DataLossPolicy::Allow),
    )
    .unwrap();
    allowing.worksheet("S").unwrap().unprotect().unwrap();
    allowing
        .write_to(Cursor::new(Vec::new()), &SaveOptions::new())
        .unwrap();
}

#[test]
fn appended_dates_and_times_keep_their_kind() {
    let row = || {
        vec![
            Value::Temporal(Temporal::date(2026, 11, 25).unwrap()),
            Value::Temporal(Temporal::datetime(2026, 11, 25, 9, 30, 0, 0).unwrap()),
            Value::Temporal(Temporal::time(9, 30, 0, 0).unwrap()),
            Value::Temporal(Temporal::duration(1, 3600, 0).unwrap()),
        ]
    };
    let kinds = [
        TemporalKind::Date,
        TemporalKind::DateTime,
        TemporalKind::Time,
        TemporalKind::Duration,
    ];
    let check = |book: &mut Workbook, row_number: u32| {
        let mut sheet = book.worksheet("S").unwrap();
        for (column, kind) in kinds.iter().enumerate() {
            let at = CellRef::new(row_number - 1, column as u32).unwrap();
            let Value::Temporal(read) = sheet.value(at).unwrap() else {
                panic!("{at} is not a date or time")
            };
            assert_eq!(read.kind(), *kind, "{at}");
        }
    };

    // A new sheet.
    let mut book = Workbook::new().unwrap();
    book.add_worksheet("S").unwrap().append_row(row()).unwrap();
    check(&mut book, 1);
    let mut book = reopen(&mut book);
    check(&mut book, 1);

    // A sheet from a file: the streamed append path.
    book.worksheet("S").unwrap().append_row(row()).unwrap();
    check(&mut book, 2);
    let mut book = reopen(&mut book);
    check(&mut book, 2);

    // A sheet from a file that a style edit has already decoded.
    let mut sheet = book.worksheet("S").unwrap();
    sheet
        .set_style("H1", &Style::new().with_number_format("0.00"))
        .unwrap();
    sheet.append_row(row()).unwrap();
    check(&mut book, 3);
    let mut book = reopen(&mut book);
    check(&mut book, 3);
}

#[test]
fn written_dates_get_a_format_only_when_the_cell_needs_one() {
    let date = Temporal::date(2026, 11, 25).unwrap();
    let datetime = Temporal::datetime(2026, 11, 25, 9, 30, 0, 0).unwrap();
    let mut book = Workbook::new().unwrap();
    let mut sheet = book.add_worksheet("S").unwrap();
    sheet.set_value("A1", 1).unwrap();
    sheet
        .set_style("B1", &Style::new().with_number_format("dd/mm/yyyy"))
        .unwrap();
    sheet
        .set_style("C1", &Style::new().with_number_format("0.00"))
        .unwrap();
    let mut book = reopen(&mut book);
    for loaded in [false, true] {
        let mut sheet = book.worksheet("S").unwrap();
        if loaded {
            // Decode the sheet first, so the second pass uses the model path.
            sheet
                .set_style("Z1", &Style::new().with_number_format("0"))
                .unwrap();
        }
        sheet.set_value("A2", date).unwrap();
        sheet.set_value("B1", date).unwrap();
        sheet.set_value("C1", datetime).unwrap();
        assert_eq!(sheet.style("B1").unwrap().number_format(), "dd/mm/yyyy");
        let mut again = reopen(&mut book);
        let mut sheet = again.worksheet("S").unwrap();
        for (at, kind) in [
            ("A2", TemporalKind::Date),
            ("B1", TemporalKind::Date),
            ("C1", TemporalKind::DateTime),
        ] {
            let Value::Temporal(read) = sheet.value(at).unwrap() else {
                panic!("{at} is not a date (loaded: {loaded})")
            };
            assert_eq!(read.kind(), kind, "{at} (loaded: {loaded})");
        }
        assert_eq!(sheet.style("B1").unwrap().number_format(), "dd/mm/yyyy");
    }
}
