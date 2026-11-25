// SPDX-License-Identifier: MIT
// Built-in table composition selected from umya-spreadsheet numbering_format.rs
// and rust_xlsxwriter format.rs; Copyright (c) 2020 MathNya;
// Copyright 2022-2026 John McNamara. Format classification is adapted from
// calamine formats.rs, Copyright 2016-2026 Johann Tuffe.
// Exact baseline codes were validated through
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

/// Classify an Excel format's first section without allocation.
/// Quoted/escaped literals and elapsed-time brackets retain the shared codec rules.
pub fn classify_number_format(format: &str) -> Option<crate::DateKind> {
    let mut escaped = false;
    let mut quote = false;
    let mut brackets = 0usize;
    let mut previous = ' ';
    let mut elapsed = false;
    let mut am_pm = false;
    for ch in format.chars() {
        match (ch, escaped, quote, am_pm, brackets) {
            (_, true, ..) => escaped = false,
            ('_' | '\\' | '*', ..) => escaped = true,
            ('"', _, true, _, _) => quote = false,
            (_, _, true, _, _) => {}
            ('"', _, _, _, _) => quote = true,
            (';', ..) => return None,
            ('[', ..) => brackets += 1,
            (']', .., 1) if elapsed => return Some(crate::DateKind::Duration),
            (']', ..) => brackets = brackets.saturating_sub(1),
            ('a' | 'A', _, _, false, 0) => am_pm = true,
            ('p' | 'm' | '/' | 'P' | 'M', _, _, true, 0) => {
                return Some(crate::DateKind::DateTime);
            }
            ('d' | 'm' | 'h' | 'y' | 's' | 'D' | 'M' | 'H' | 'Y' | 'S', _, _, false, 0) => {
                return Some(crate::DateKind::DateTime);
            }
            _ => {
                if !(elapsed && ch.eq_ignore_ascii_case(&previous)) {
                    elapsed = previous == '[' && matches!(ch, 'm' | 'h' | 's' | 'M' | 'H' | 'S');
                }
            }
        }
        previous = ch;
    }
    None
}

/// Classify a format's first section by the parts it shows: [`DateKind::Date`]
/// for date parts only, [`DateKind::Time`] for time parts only,
/// [`DateKind::DateTime`] for both and [`DateKind::Duration`] for elapsed
/// time such as `[h]:mm`. Returns `None` for non-temporal formats.
///
/// `m` means minutes after an hour or before a second, and months otherwise.
///
/// [`DateKind::Date`]: crate::DateKind::Date
/// [`DateKind::Time`]: crate::DateKind::Time
/// [`DateKind::DateTime`]: crate::DateKind::DateTime
/// [`DateKind::Duration`]: crate::DateKind::Duration
pub fn classify_temporal_format(format: &str) -> Option<crate::DateKind> {
    use crate::DateKind;
    if classify_number_format(format)? == DateKind::Duration {
        return Some(DateKind::Duration);
    }
    // Significant letters of the first section, outside quotes and escapes.
    let mut letters = Vec::new();
    let mut escaped = false;
    let mut quote = false;
    let mut brackets = 0usize;
    for ch in format.chars() {
        match (ch, escaped, quote) {
            (_, true, _) => escaped = false,
            ('"', _, true) => quote = false,
            (_, _, true) => {}
            ('"', _, _) => quote = true,
            ('_' | '\\' | '*', _, _) => escaped = true,
            (';', _, _) => break,
            ('[', _, _) => brackets += 1,
            (']', _, _) => brackets = brackets.saturating_sub(1),
            (_, _, _) if brackets > 0 => {}
            (c, _, _) if c.is_ascii_alphabetic() => letters.push(c.to_ascii_lowercase()),
            _ => {}
        }
    }
    let text: String = letters.iter().collect();
    let mut date = false;
    let mut time = text.contains("am/pm")
        || text.contains("a/p")
        || text.contains("ampm")
        || text.contains("ap");
    let significant: Vec<char> = letters
        .iter()
        .copied()
        .filter(|c| matches!(c, 'd' | 'y' | 'm' | 'h' | 's'))
        .collect();
    for (index, letter) in significant.iter().enumerate() {
        match letter {
            'd' | 'y' => date = true,
            'h' | 's' => time = true,
            'm' => {
                let before = significant[..index].iter().rev().find(|c| **c != 'm');
                let after = significant[index + 1..].iter().find(|c| **c != 'm');
                if before == Some(&'h') || after == Some(&'s') {
                    time = true;
                } else {
                    date = true;
                }
            }
            _ => {}
        }
    }
    match (date, time) {
        (true, false) => Some(DateKind::Date),
        (false, true) => Some(DateKind::Time),
        _ => Some(DateKind::DateTime),
    }
}

#[cfg(test)]
mod temporal_tests {
    use super::*;
    use crate::DateKind;

    #[test]
    fn formats_classify_by_the_parts_they_show() {
        for (format, kind) in [
            ("yyyy-mm-dd", DateKind::Date),
            ("d-mmm-yy", DateKind::Date),
            ("mmm-yy", DateKind::Date),
            ("h:mm:ss", DateKind::Time),
            ("hh:mm AM/PM", DateKind::Time),
            ("mm:ss", DateKind::Time),
            ("yyyy-mm-dd hh:mm", DateKind::DateTime),
            ("m/d/yy h:mm", DateKind::DateTime),
            ("[h]:mm:ss", DateKind::Duration),
            ("\"Day\" dd", DateKind::Date),
        ] {
            assert_eq!(classify_temporal_format(format), Some(kind), "{format}");
        }
        assert_eq!(classify_temporal_format("0.00"), None);
        assert_eq!(classify_temporal_format("General"), None);
    }
}
