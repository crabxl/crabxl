//! Aggregate owned-workbook allowances and stable identity behavior.
#![allow(clippy::unwrap_used)]
use crabxl_core::{
    Cell, CellAddress, CellValue, DateEpoch, EditLimits, ErrorKind, RowIndex, StyleId, Workbook,
    WorkbookLimits,
};
fn cell(row: u32, value: i64) -> Cell {
    Cell {
        address: CellAddress::new(row, 0).unwrap(),
        value: CellValue::Integer(value),
        style: StyleId::new(0),
    }
}
#[test]
fn sheet_ids_survive_copy_reorder_rename_and_reject_removed_foreign_handles() {
    let mut book = Workbook::new(WorkbookLimits::default()).unwrap();
    let first = book.create_sheet("First").unwrap();
    let second = book.create_sheet("Second").unwrap();
    book.sheet_mut(first).unwrap().set(cell(0, 1)).unwrap();
    book.sheet_mut(first).unwrap().append(vec![]).unwrap();
    book.sheet_mut(first)
        .unwrap()
        .append(vec![CellValue::Integer(3)])
        .unwrap();
    let copied = book.copy_sheet(first, "Copy").unwrap();
    assert_eq!(book.sheet(copied).unwrap().row_extent(), 3);
    book.sheet_mut(copied).unwrap().set(cell(0, 7)).unwrap();
    assert_eq!(
        book.sheet(first)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap()
            .value,
        CellValue::Integer(1)
    );
    book.rename_sheet(first, "Renamed").unwrap();
    book.set_active_sheet(copied).unwrap();
    book.move_sheet(copied, 0).unwrap();
    assert_eq!(book.active_index(), Some(0));
    assert_eq!(book.sheet_id("Renamed"), Some(first));
    let transferred = book.remove_sheet(second).unwrap();
    assert_eq!(transferred.name(), "Second");
    assert_eq!(
        book.sheet(second).err().unwrap().kind(),
        ErrorKind::SheetNotFound
    );
    let new = book.create_sheet("Second").unwrap();
    assert_ne!(new, second);
    let other = Workbook::new(WorkbookLimits::default()).unwrap();
    assert_eq!(
        other.sheet(first).err().unwrap().kind(),
        ErrorKind::SheetNotFound
    );
    assert!(book.rename_sheet(new, "renamed").is_err());
    assert_eq!(book.sheet(new).unwrap().name(), "Second");
    book.set_epoch(DateEpoch::Mac1904);
    assert_eq!(book.epoch(), DateEpoch::Mac1904);
}
#[test]
fn aggregate_bytes_cells_and_work_budget_fail_without_changing_models() {
    let mut book = Workbook::new(WorkbookLimits {
        max_bytes: 1200,
        max_cells: 2,
        max_sheets: 4,
        sheet: EditLimits {
            max_bytes: 1000,
            max_cells: 2,
        },
    })
    .unwrap();
    let a = book.create_sheet("A").unwrap();
    let b = book.create_sheet("B").unwrap();
    book.sheet_mut(a).unwrap().set(cell(0, 1)).unwrap();
    book.sheet_mut(b).unwrap().set(cell(0, 2)).unwrap();
    let bytes = book.charged_bytes();
    assert!(bytes <= 1200);
    assert_eq!(
        book.sheet_mut(a)
            .unwrap()
            .set(cell(1, 3))
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(book.cell_count(), 2);
    assert_eq!(book.charged_bytes(), bytes);
    assert!(
        book.sheet_mut(a)
            .unwrap()
            .insert_rows(RowIndex::new(0).unwrap(), 1)
            .is_err()
    );
    assert!(
        book.sheet(a)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .is_some()
    );
    assert!(book.copy_sheet(a, "Copy").is_err());
    assert_eq!(book.len(), 2);
    assert!(book.rename_sheet(a, "x".repeat(1000)).is_err());
    assert_eq!(book.sheet(a).unwrap().name(), "A");
    book.sheet_mut(b)
        .unwrap()
        .remove(CellAddress::new(0, 0).unwrap());
    book.sheet_mut(a).unwrap().set(cell(1, 3)).unwrap();
    assert_eq!(book.cell_count(), 2);
}
#[test]
fn freed_space_is_reusable_and_slot_capacity_remains_accounted() {
    let mut book = Workbook::new(WorkbookLimits {
        max_bytes: 1600,
        max_cells: 10,
        max_sheets: 2,
        ..WorkbookLimits::default()
    })
    .unwrap();
    let a = book.create_sheet("A").unwrap();
    book.sheet_mut(a)
        .unwrap()
        .append(vec![CellValue::Integer(1), CellValue::Integer(2)])
        .unwrap();
    let b = book.create_sheet("B").unwrap();
    assert_eq!(book.charged_bytes(), 1026);
    assert_eq!(
        book.create_sheet("C").unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
    book.remove_sheet(a).unwrap();
    assert_eq!(book.charged_bytes(), 513);
    book.sheet_mut(b)
        .unwrap()
        .append(vec![
            CellValue::Integer(1),
            CellValue::Integer(2),
            CellValue::Integer(3),
            CellValue::Integer(4),
        ])
        .unwrap();
    assert_eq!(book.charged_bytes(), 1537);
    assert_eq!(book.active_sheet(), Some(b));
}

#[test]
fn themes_share_immutable_bytes_and_obey_the_bank_allowance_atomically() {
    use crabxl_core::Theme;
    let mut book = Workbook::new(WorkbookLimits {
        max_bytes: 8192,
        ..Default::default()
    })
    .unwrap();
    let id = book.create_sheet("Sheet").unwrap();
    let base = book.charged_bytes();
    let theme = Theme::from_bytes(vec![b'x'; 4096].into_boxed_slice());
    let shared = theme.clone();
    assert!(std::ptr::eq(
        theme.bytes().as_ptr(),
        shared.bytes().as_ptr()
    ));
    book.set_theme(Some(theme)).unwrap();
    assert_eq!(book.charged_bytes(), base + shared.memory_bytes());
    let before = book.charged_bytes();
    assert_eq!(
        book.set_theme(Some(Theme::from_bytes(vec![b'y'; 8192].into_boxed_slice())))
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(book.charged_bytes(), before);
    assert_eq!(book.theme().unwrap().bytes(), shared.bytes());
    assert!(
        book.sheet_mut(id)
            .unwrap()
            .set(Cell {
                address: CellAddress::new(0, 0).unwrap(),
                value: CellValue::text("z".repeat(5000)),
                style: StyleId::new(0)
            })
            .is_err()
    );
    assert!(book.copy_sheet(id, "Copy").is_ok());
    book.set_theme(None).unwrap();
    assert!(book.theme().is_none());
    book.sheet_mut(id).unwrap().set(cell(0, 7)).unwrap();
}

#[test]
fn canonical_styles_share_bank_budgets_and_transfer_without_payload_clones() {
    use crabxl_core::{CellStyle, StyleLimits, StyleRegistry, Theme};
    let mut source = StyleRegistry::new(StyleLimits::default()).unwrap();
    let mut style = CellStyle::default();
    style.font.name = Some("Imported".into());
    style.number_format = "0.000".into();
    let id = source.register(style.clone()).unwrap();
    let catalog = source.catalog().clone();
    let font_pointer = catalog.fonts[1].name.as_ref().unwrap().as_ptr();
    let mut bank = Workbook::new(WorkbookLimits::default()).unwrap();
    let sheet = bank.create_sheet("Imported").unwrap();
    let mut styled = cell(0, 1);
    styled.style = id;
    bank.sheet_mut(sheet).unwrap().set(styled).unwrap();
    bank.import_style_catalog(catalog, StyleLimits::default())
        .unwrap();
    assert_eq!(bank.register_style(style).unwrap(), id);
    assert_eq!(
        bank.style_catalog().unwrap().fonts[1]
            .name
            .as_ref()
            .unwrap()
            .as_ptr(),
        font_pointer
    );
    assert!(
        bank.import_style_catalog(source.catalog().clone(), StyleLimits::default())
            .is_err()
    );
    let mut tight = Workbook::new(WorkbookLimits {
        max_bytes: bank.charged_bytes(),
        ..Default::default()
    })
    .unwrap();
    let tight_sheet = tight.create_sheet("Imported").unwrap();
    let mut styled = cell(0, 1);
    styled.style = id;
    tight.sheet_mut(tight_sheet).unwrap().set(styled).unwrap();
    tight
        .import_style_catalog(source.catalog().clone(), StyleLimits::default())
        .unwrap();
    let before = tight.charged_bytes();
    assert!(
        tight
            .set_theme(Some(Theme::from_bytes(vec![1; 4096].into_boxed_slice())))
            .is_err()
    );
    assert!(
        tight
            .sheet_mut(tight_sheet)
            .unwrap()
            .set(cell(1, 2))
            .is_err()
    );
    assert_eq!(tight.charged_bytes(), before);
    assert_eq!(tight.cell_count(), 1);
    let mut parts = bank.into_parts();
    assert_eq!(parts.active_sheet, Some(0));
    assert_eq!(
        parts.styles.as_ref().unwrap().catalog().fonts[1]
            .name
            .as_ref()
            .unwrap()
            .as_ptr(),
        font_pointer
    );
    assert_eq!(parts.sheets.len(), 1);
    assert_eq!(
        parts
            .sheets
            .next()
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap()
            .style,
        id
    );
    assert!(parts.sheets.next().is_none());
}

#[test]
fn failed_style_import_and_registration_leave_owned_cells_usable() {
    use crabxl_core::{CellStyle, StyleLimits, StyleRegistry};
    let mut bank = Workbook::new(WorkbookLimits::default()).unwrap();
    let sheet = bank.create_sheet("Sheet").unwrap();
    let mut wrong = cell(0, 1);
    wrong.style = StyleId::new(100);
    bank.sheet_mut(sheet).unwrap().set(wrong).unwrap();
    let source = StyleRegistry::new(StyleLimits::default()).unwrap();
    assert!(
        bank.import_style_catalog(source.catalog().clone(), StyleLimits::default())
            .is_err()
    );
    assert!(bank.style_catalog().is_none());
    bank.sheet_mut(sheet).unwrap().set(cell(0, 1)).unwrap();
    let mut invalid = CellStyle::default();
    invalid.font.size = Some(f64::NAN);
    assert!(bank.register_style(invalid).is_err());
    assert!(bank.style_catalog().is_none());
    assert_eq!(
        bank.register_style(CellStyle::default()).unwrap(),
        StyleId::new(0)
    );
    assert_eq!(bank.cell_count(), 1);
}

#[test]
fn imported_bank_raw_format_edits_reuse_components_and_source_identities() {
    use crabxl_core::{CellStyle, StyleLimits, StyleRegistry};
    let mut bank = Workbook::new(WorkbookLimits::default()).unwrap();
    assert!(bank.register_number_format("0.00".into()).is_err());
    let mut source = StyleRegistry::new(StyleLimits::default()).unwrap();
    let mut style = CellStyle::default();
    style.font.name = Some("SharedComponent".into());
    let id = source.register(style).unwrap();
    bank.import_style_catalog(source.catalog().clone(), StyleLimits::default())
        .unwrap();
    let pointer = bank.style_catalog().unwrap().fonts[1]
        .name
        .as_ref()
        .unwrap()
        .as_ptr();
    let mut format = bank
        .style_catalog()
        .unwrap()
        .cell_format(id)
        .unwrap()
        .clone();
    format.number_format_id = bank.register_number_format("0.0000".into()).unwrap();
    let edited = bank.register_format(format.clone()).unwrap();
    assert_eq!(bank.register_format(format).unwrap(), edited);
    let catalog = bank.style_catalog().unwrap();
    assert_eq!(catalog.fonts.len(), 2);
    assert_eq!(catalog.fonts[1].name.as_ref().unwrap().as_ptr(), pointer);
    assert_eq!(
        catalog.cell_style(id).unwrap().number_format,
        Some("General")
    );
    assert_eq!(
        catalog.cell_style(edited).unwrap().number_format,
        Some("0.0000")
    );
}

#[test]
fn releasing_sheet_storage_restores_style_registration_allowance() {
    use crabxl_core::{CellStyle, StyleLimits, StyleRegistry};
    let mut bank = Workbook::new(WorkbookLimits {
        max_bytes: 64 * 1024,
        ..Default::default()
    })
    .unwrap();
    let sheet = bank.create_sheet("Large").unwrap();
    bank.sheet_mut(sheet)
        .unwrap()
        .set(Cell {
            address: CellAddress::new(0, 0).unwrap(),
            value: CellValue::text("x".repeat(48 * 1024)),
            style: StyleId::new(0),
        })
        .unwrap();
    let source = StyleRegistry::new(StyleLimits::default()).unwrap();
    bank.import_style_catalog(source.catalog().clone(), StyleLimits::default())
        .unwrap();
    let mut appearance = CellStyle::default();
    appearance.font.name = Some("y".repeat(20 * 1024).into());
    assert!(bank.register_style(appearance.clone()).is_err());
    let removed = bank.remove_sheet(sheet).unwrap();
    drop(removed);
    let id = bank.register_style(appearance).unwrap();
    assert_eq!(
        bank.style_catalog()
            .unwrap()
            .cell_style(id)
            .unwrap()
            .font
            .name
            .as_ref()
            .unwrap()
            .len(),
        20 * 1024
    );
    assert!(bank.charged_bytes() <= 64 * 1024);
}
