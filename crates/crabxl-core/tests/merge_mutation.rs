//! Raw metadata and realized sparse appearance edits under shared allowances.
#![allow(clippy::unwrap_used)]
use crabxl_core::{CellAddress, CellRange, ErrorKind, MergedCellRange, MergedRanges, StyleId};

#[test]
fn raw_replacement_retains_identities_virtual_cells_and_rejects_budget_failure() {
    let mut merges = MergedRanges::default();
    let first: CellRange = "A1:XFD1048576".parse().unwrap();
    merges
        .insert_with_limit(
            MergedCellRange::new(first, [StyleId::new(1); 16]).unwrap(),
            65536,
        )
        .unwrap();
    let identity = merges.ranges()[0].identity();
    let bytes = merges.heap_bytes();
    assert_eq!(
        merges
            .replace_declarations(&["B2:C3".parse().unwrap()], bytes)
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(merges.declaration(identity).unwrap().range(), first);
    assert_eq!(
        merges.virtual_style("XFD1048576".parse().unwrap()),
        Some(StyleId::new(1))
    );
    merges
        .replace_declarations(&[first, "B2:C3".parse().unwrap(), first], 65536)
        .unwrap();
    assert_eq!(merges.ranges().len(), 2);
    assert_eq!(merges.declaration(identity).unwrap().range(), first);
    merges.replace_declarations(&[], 65536).unwrap();
    assert!(merges.ranges().is_empty());
    assert!(merges.declaration(identity).is_none());
    assert_eq!(
        merges.virtual_style("XFD1048576".parse().unwrap()),
        Some(StyleId::new(1))
    );
    assert!(merges.heap_bytes() < 4096);
    assert!(
        merges
            .add_exact_declaration("J1:K2".parse().unwrap(), 65536)
            .unwrap()
    );
    assert!(
        merges
            .add_exact_declaration("J1:J2".parse().unwrap(), 65536)
            .unwrap()
    );
    assert!(
        !merges
            .add_exact_declaration("J1:K2".parse().unwrap(), 65536)
            .unwrap()
    );
    assert_eq!(merges.ranges().len(), 2);
    assert_eq!(
        merges.virtual_style(CellAddress::new(1, 1).unwrap()),
        Some(StyleId::new(1))
    );
}
