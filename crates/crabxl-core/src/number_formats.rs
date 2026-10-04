// SPDX-License-Identifier: MIT
// Built-in table composition selected from umya-spreadsheet numbering_format.rs
// and rust_xlsxwriter format.rs; Copyright (c) 2020 MathNya;
// Copyright 2022-2026 John McNamara. Exact baseline codes were validated through
// the public openpyxl 3.1.5 BUILTIN_FORMATS mapping, without reading its source.

/// Known portable built-in format identities and reference-compatible spelling.
/// Locale-specific reserved identities are deliberately not fabricated.
pub const BUILTIN_NUMBER_FORMATS: &[(u32, &str)] = &[
    (0, "General"),
    (1, "0"),
    (2, "0.00"),
    (3, "#,##0"),
    (4, "#,##0.00"),
    (5, "\"$\"#,##0_);(\"$\"#,##0)"),
    (6, "\"$\"#,##0_);[Red](\"$\"#,##0)"),
    (7, "\"$\"#,##0.00_);(\"$\"#,##0.00)"),
    (8, "\"$\"#,##0.00_);[Red](\"$\"#,##0.00)"),
    (9, "0%"),
    (10, "0.00%"),
    (11, "0.00E+00"),
    (12, "# ?/?"),
    (13, "# ??/??"),
    (14, "mm-dd-yy"),
    (15, "d-mmm-yy"),
    (16, "d-mmm"),
    (17, "mmm-yy"),
    (18, "h:mm AM/PM"),
    (19, "h:mm:ss AM/PM"),
    (20, "h:mm"),
    (21, "h:mm:ss"),
    (22, "m/d/yy h:mm"),
    (37, "#,##0_);(#,##0)"),
    (38, "#,##0_);[Red](#,##0)"),
    (39, "#,##0.00_);(#,##0.00)"),
    (40, "#,##0.00_);[Red](#,##0.00)"),
    (41, "_(* #,##0_);_(* \\(#,##0\\);_(* \"-\"_);_(@_)"),
    (
        42,
        "_(\"$\"* #,##0_);_(\"$\"* \\(#,##0\\);_(\"$\"* \"-\"_);_(@_)",
    ),
    (43, "_(* #,##0.00_);_(* \\(#,##0.00\\);_(* \"-\"??_);_(@_)"),
    (
        44,
        "_(\"$\"* #,##0.00_)_(\"$\"* \\(#,##0.00\\)_(\"$\"* \"-\"??_)_(@_)",
    ),
    (45, "mm:ss"),
    (46, "[h]:mm:ss"),
    (47, "mmss.0"),
    (48, "##0.0E+0"),
    (49, "@"),
];

/// Borrow a known built-in format without allocating a table or format string.
pub fn builtin_number_format(id: u32) -> Option<&'static str> {
    BUILTIN_NUMBER_FORMATS
        .binary_search_by_key(&id, |(key, _)| *key)
        .ok()
        .map(|index| BUILTIN_NUMBER_FORMATS[index].1)
}
/// Find a portable built-in identity by its exact baseline spelling.
/// Different locale spellings remain explicit custom format codes.
pub fn builtin_number_format_id(code: &str) -> Option<u32> {
    BUILTIN_NUMBER_FORMATS
        .iter()
        .find(|(_, value)| *value == code)
        .map(|(id, _)| *id)
}
