//! Formula-offset baseline cases and atomic sparse-model integration.
#![allow(clippy::unwrap_used)]
use crabxl_core::{
    Cell, CellAddress, CellRange, CellValue, EditLimits, ErrorKind, Formula, StyleId, Worksheet,
    formula_position, translate_axis, translate_expression,
};
#[test]
fn reference_context_quotes_tables_axis_ranges_and_errors() {
    use crabxl_core::tokenize_formula;
    for (source, expected) in [
        (
            "=IF(A1>=2,\"a\"\"b\",#N/A)",
            vec![
                ("IF(", "FUNC", "OPEN"),
                ("A1", "OPERAND", "RANGE"),
                (">=", "OPERATOR-INFIX", ""),
                ("2", "OPERAND", "NUMBER"),
                (",", "SEP", "ARG"),
                ("\"a\"\"b\"", "OPERAND", "TEXT"),
                (",", "SEP", "ARG"),
                ("#N/A", "OPERAND", "ERROR"),
                (")", "FUNC", "CLOSE"),
            ],
        ),
        (
            "=-1.2E-3%+TRUE",
            vec![
                ("-", "OPERATOR-PREFIX", ""),
                ("1.2E-3", "OPERAND", "NUMBER"),
                ("%", "OPERATOR-POSTFIX", ""),
                ("+", "OPERATOR-INFIX", ""),
                ("TRUE", "OPERAND", "LOGICAL"),
            ],
        ),
        (
            "={1,2;3,4}",
            vec![
                ("{", "ARRAY", "OPEN"),
                ("1", "OPERAND", "NUMBER"),
                (",", "SEP", "ARG"),
                ("2", "OPERAND", "NUMBER"),
                (";", "SEP", "ROW"),
                ("3", "OPERAND", "NUMBER"),
                (",", "SEP", "ARG"),
                ("4", "OPERAND", "NUMBER"),
                ("}", "ARRAY", "CLOSE"),
            ],
        ),
        (
            "='\u{8cc7}\u{6599}''\u{8868}'!A1+T[[#Headers],[B]]",
            vec![
                ("'\u{8cc7}\u{6599}''\u{8868}'!A1", "OPERAND", "RANGE"),
                ("+", "OPERATOR-INFIX", ""),
                ("T[[#Headers],[B]]", "OPERAND", "RANGE"),
            ],
        ),
        (
            "=a,b",
            vec![
                ("a", "OPERAND", "RANGE"),
                (",", "OPERATOR-INFIX", ""),
                ("b", "OPERAND", "RANGE"),
            ],
        ),
        ("literal", vec![("literal", "LITERAL", "")]),
        ("", vec![]),
        ("=", vec![]),
    ] {
        let tokens = tokenize_formula(source, 4096).unwrap();
        let actual = tokens
            .iter()
            .map(|t| (t.value.as_ref(), t.kind.as_str(), t.subtype.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{source}");
        for token in &tokens {
            // Non-whitespace spelling is borrowed from the original source.
            assert!(token.value.as_ptr() >= source.as_ptr());
            assert!(
                (token.value.as_ptr() as usize) + token.value.len()
                    <= (source.as_ptr() as usize) + source.len()
            );
        }
    }
    let cases = [
        ("=A1+$B2+C$3+$D$4", "=B2+$B3+D$3+$D$4"),
        ("='A1'!A1+A1!B2", "='A1'!B2+A1!C3"),
        ("='it''s A1'!A1", "='it''s A1'!B2"),
        (
            "=T1[A1]+T2[[#Headers],[B2]]+A1",
            "=T1[A1]+T2[[#Headers],[B2]]+B2",
        ),
        (
            "=\"A1\"&\"say \"\"B2\"\"\"+A1",
            "=\"A1\"&\"say \"\"B2\"\"\"+B2",
        ),
        ("=LOG10(A1)+SUM(A1:B2:C3)", "=LOG10(B2)+SUM(B2:C3:D4)"),
        ("=1:2+A:B+$1:$2+$A:$B", "=2:3+B:C+$1:$2+$A:$B"),
        ("=#REF!+#DIV/0!+#N/A+A1", "=#REF!+#DIV/0!+#N/A+B2"),
        ("=XFD1048576", "=XFE1048577"),
        ("=AA1001001001+R1C1+named1", "=AA1001001001+R1C1+named1"),
        ("=@A1+A01+éA1+øA1", "=@A1+A01+éA1+øA1"),
        ("=[1]A1!B2", "=[1]A1!C3"),
    ];
    for (source, expected) in cases {
        assert_eq!(
            translate_expression(source, 1, 1, 4096).unwrap(),
            expected,
            "{source}"
        );
    }
    assert_eq!(formula_position("AA1001001001").unwrap(), (1001001001, 27));
    assert_eq!(translate_axis("1001001001", 1, true).unwrap(), "1001001002");
    assert_eq!(translate_axis("$a", 10, false).unwrap(), "$a");
    let generated = tokenize_formula("=OFFSET(A1,0,0):C3", 4096).unwrap();
    let closer = generated
        .iter()
        .position(|t| t.subtype == crabxl_core::TokenSubtype::Close)
        .unwrap();
    assert_eq!(generated[closer + 1].value, ":");
    assert_eq!(generated[closer + 1].kind, crabxl_core::TokenKind::Infix);
    assert_eq!(generated[closer + 2].value, "C3");
    for (source, expected) in [
        (
            "=A1\nB1",
            vec![("\n", "WHITE-SPACE", ""), ("A1B1", "OPERAND", "RANGE")],
        ),
        (
            "=SUM\n(A1)",
            vec![
                ("\n", "WHITE-SPACE", ""),
                ("SUM(", "FUNC", "OPEN"),
                ("A1", "OPERAND", "RANGE"),
                (")", "FUNC", "CLOSE"),
            ],
        ),
        (
            "=A1\n+\tB2",
            vec![
                ("\n", "WHITE-SPACE", ""),
                ("A1", "OPERAND", "RANGE"),
                ("+", "OPERATOR-INFIX", ""),
                ("\tB2", "OPERAND", "RANGE"),
            ],
        ),
    ] {
        let tokens = tokenize_formula(source, 4096).unwrap();
        assert_eq!(
            tokens
                .iter()
                .map(|t| (t.value.as_ref(), t.kind.as_str(), t.subtype.as_str()))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t.value, std::borrow::Cow::Owned(_)))
        );
    }
}
#[test]
fn invalid_boundaries_syntax_and_output_capacity_do_not_return_partial_results() {
    for source in [
        "=\"text",
        "='Sheet!A1",
        "=Table[A1",
        "=1)",
        "={1)",
        "=#SPILL!",
        "=A1#",
    ] {
        assert_eq!(
            crabxl_core::tokenize_formula(source, 4096)
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
    }
    for source in ["literal", "=1+2", "=SUM(1,2)"] {
        assert_eq!(
            crabxl_core::tokenize_formula(source, 2).unwrap_err().kind(),
            ErrorKind::MemoryBudgetExceeded
        );
    }
    assert!(crabxl_core::tokenize_formula("=SUM(1", 4096).is_ok());
    for (source, row, col) in [
        ("=A1", -1, 0),
        ("=A1", 0, -1),
        ("=ZZZ1", 0, 1),
        ("=A1", 0, i64::MAX),
    ] {
        assert_eq!(
            translate_expression(source, row, col, 4096)
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
    }
    assert_eq!(
        translate_expression("=Z9", 1, 1, 3).unwrap_err().kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(
        translate_expression("=A1#", 1, 1, 4096).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    for source in ["=\"A1", "='Sheet!A1", "=Table[A1"] {
        assert_eq!(
            translate_expression(source, 1, 1, 4096).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }
}
#[test]
fn translated_model_moves_validate_all_formulas_before_mutation_and_drop_caches() {
    let mut sheet = Worksheet::new("Sheet", EditLimits::default()).unwrap();
    let cell = |row, col, expression| Cell {
        address: CellAddress::new(row, col).unwrap(),
        value: CellValue::Formula(Box::new(
            Formula::new(expression, Some(CellValue::Integer(1))).unwrap(),
        )),
        style: StyleId::new(2),
    };
    sheet.set(cell(1, 1, "=C3+$D$4")).unwrap();
    sheet.set(cell(1, 2, "=A1")).unwrap();
    sheet.mark_clean();
    let before = sheet.charged_bytes();
    let range = CellRange::new(
        CellAddress::new(1, 1).unwrap(),
        CellAddress::new(1, 2).unwrap(),
    )
    .unwrap();
    assert!(sheet.move_range_translated(range, -1, 0).is_err());
    assert!(!sheet.is_dirty());
    assert_eq!(sheet.charged_bytes(), before);
    assert!(sheet.get(CellAddress::new(1, 1).unwrap()).is_some());
    sheet.move_range_translated(range, 1, 1).unwrap();
    let moved = sheet.get(CellAddress::new(2, 2).unwrap()).unwrap();
    assert_eq!(moved.style, StyleId::new(2));
    let CellValue::Formula(formula) = &moved.value else {
        panic!("Missing moved formula");
    };
    assert_eq!(formula.expression(), "D4+$D$4");
    assert!(formula.cached().is_none());
}

#[test]
fn translated_move_memory_failure_retains_original_coordinates_values_and_cache() {
    let mut sheet = Worksheet::new(
        "Sheet",
        EditLimits {
            max_bytes: 600,
            max_cells: 1,
        },
    )
    .unwrap();
    let address = CellAddress::new(0, 0).unwrap();
    let value = CellValue::Formula(Box::new(
        Formula::new("=Z9", Some(CellValue::Integer(1))).unwrap(),
    ));
    sheet
        .set(Cell {
            address,
            value: value.clone(),
            style: StyleId::new(0),
        })
        .unwrap();
    sheet.mark_clean();
    let before = sheet.charged_bytes();
    assert_eq!(
        sheet
            .move_range_translated(CellRange::new(address, address).unwrap(), 1, 1)
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(sheet.get(address).unwrap().value, value);
    assert_eq!(sheet.charged_bytes(), before);
    assert!(!sheet.is_dirty());
}
