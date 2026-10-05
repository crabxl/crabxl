//! Shared identifiers preserve public spelling and shared allocation ownership.
use crabxl_core::{FormulaMetadata, FormulaType, SharedFormulaIndex};
use std::sync::Arc;

#[test]
fn canonical_unsigned_ids_need_no_string_and_other_spellings_remain_distinct() {
    for (text, number) in [("0", 0), ("1", 1), ("4294967295", u32::MAX)] {
        let index = SharedFormulaIndex::from_literal(text);
        assert_eq!(index, SharedFormulaIndex::Numeric(number));
        assert_eq!(index.heap_bytes(), 0);
        assert_eq!(index.to_string(), text);
    }
    for text in [
        "",
        "01",
        "+1",
        "-1",
        "4294967296",
        "text",
        "1 ",
        "quoted & value",
    ] {
        let index = SharedFormulaIndex::from_literal(text);
        assert_eq!(index.literal(), Some(text));
        assert_eq!(index.to_string(), text);
        assert_eq!(index.heap_bytes(), text.len() + 2 * size_of::<usize>());
    }
    assert_ne!(
        SharedFormulaIndex::Missing,
        SharedFormulaIndex::from_literal("")
    );
    assert_ne!(SharedFormulaIndex::from_literal("01"), 1.into());
}

#[test]
fn metadata_clones_share_index_payload_and_account_the_retained_header() {
    let metadata = FormulaMetadata {
        kind: FormulaType::Shared {
            index: SharedFormulaIndex::from_literal("opaque"),
            master: true,
        },
        ..Default::default()
    };
    let cloned = metadata.clone();
    let (
        FormulaType::Shared {
            index: SharedFormulaIndex::Literal(first),
            ..
        },
        FormulaType::Shared {
            index: SharedFormulaIndex::Literal(second),
            ..
        },
    ) = (&metadata.kind, &cloned.kind)
    else {
        panic!("Expected literal shared identities");
    };
    assert!(Arc::ptr_eq(first, second));
    assert_eq!(metadata.payload_bytes(), "opaque".len());
    assert_eq!(
        metadata.memory_bytes(),
        size_of::<FormulaMetadata>() + "opaque".len() + 2 * size_of::<usize>()
    );
}
