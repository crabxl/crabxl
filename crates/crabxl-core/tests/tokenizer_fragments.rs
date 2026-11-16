//! Explicit scanner fragments share the bounded formula lexer primitives.
use crabxl_core::{FormulaScan, TokenKind, scan_formula_fragment};

#[test]
fn fragments_use_character_offsets_and_preserve_source_spelling() -> crabxl_core::Result<()> {
    for (source, operation, expected) in [
        ("α\"中文\"尾", FormulaScan::Quoted, "\"中文\""),
        ("α[中[文]]尾", FormulaScan::Brackets, "[中[文]]"),
        ("α#REF!尾", FormulaScan::Error, "#REF!"),
    ] {
        let (token, consumed) = scan_formula_fragment(source, 1, operation, "", false, None, 1024)?;
        assert_eq!(token.value, expected);
        assert_eq!(consumed, expected.chars().count());
    }
    Ok(())
}

#[test]
fn fragment_validation_rejects_bad_offsets_context_and_joint_budget() -> crabxl_core::Result<()> {
    assert!(scan_formula_fragment("[a]", 4, FormulaScan::Brackets, "", false, None, 1024).is_err());
    assert!(scan_formula_fragment("[a", 0, FormulaScan::Brackets, "", false, None, 1024).is_err());
    assert!(
        scan_formula_fragment(
            ")",
            0,
            FormulaScan::Closer,
            "",
            false,
            Some(TokenKind::Operand),
            1024
        )
        .is_err()
    );
    assert!(scan_formula_fragment("(", 0, FormulaScan::Opener, "name", false, None, 4).is_err());
    let (token, consumed) =
        scan_formula_fragment("(", 0, FormulaScan::Opener, "name", false, None, 5)?;
    assert_eq!(token.value, "name(");
    assert_eq!(token.kind, TokenKind::Function);
    assert_eq!(consumed, 1);
    Ok(())
}

#[test]
fn spill_references_are_operands_and_unfinished_errors_still_fail() -> crabxl_core::Result<()> {
    let tokens = crabxl_core::tokenize_formula("=SUM(A1#)+Sheet1!$B$2#", 4096)?;
    let operands: Vec<_> = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Operand)
        .map(|token| token.value.as_ref())
        .collect();
    assert_eq!(operands, ["A1#", "Sheet1!$B$2#"]);
    assert!(crabxl_core::tokenize_formula("=A1#REF!", 4096).is_err());
    Ok(())
}
