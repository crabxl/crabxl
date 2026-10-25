//! Workbook and sheet-scoped defined names.
//!
//! Modelled after openpyxl `workbook/defined_name.py`; the print area and print
//! titles helpers produce the same text as openpyxl `Worksheet.print_area` and
//! `Worksheet.print_titles` (openpyxl, MIT).

/// Name of the built-in print area defined name.
pub const PRINT_AREA_NAME: &str = "_xlnm.Print_Area";
/// Name of the built-in print titles defined name.
pub const PRINT_TITLES_NAME: &str = "_xlnm.Print_Titles";
/// Name of the built-in auto filter defined name.
pub const FILTER_DATABASE_NAME: &str = "_xlnm._FilterDatabase";

/// One `<definedName>`: a named formula, constant or range.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DefinedName {
    /// Name, unique per scope (built-ins use the `_xlnm.` prefix).
    pub name: Box<str>,
    /// Formula or range text (`attr_text` / `value` in openpyxl).
    pub value: Box<str>,
    /// `comment` attribute.
    pub comment: Option<Box<str>>,
    /// `customMenu` attribute.
    pub custom_menu: Option<Box<str>>,
    /// `description` attribute.
    pub description: Option<Box<str>>,
    /// `help` attribute.
    pub help: Option<Box<str>>,
    /// `statusBar` attribute.
    pub status_bar: Option<Box<str>>,
    /// Zero-based index of the sheet the name is local to; `None` is global.
    pub local_sheet_id: Option<u32>,
    /// `hidden` attribute.
    pub hidden: Option<bool>,
    /// `function` attribute.
    pub function: Option<bool>,
    /// `vbProcedure` attribute.
    pub vb_procedure: Option<bool>,
    /// `xlm` attribute.
    pub xlm: Option<bool>,
    /// `functionGroupId` attribute.
    pub function_group_id: Option<u32>,
    /// `shortcutKey` attribute.
    pub shortcut_key: Option<Box<str>>,
    /// `publishToServer` attribute.
    pub publish_to_server: Option<bool>,
    /// `workbookParameter` attribute.
    pub workbook_parameter: Option<bool>,
}

impl DefinedName {
    /// A global name with the given formula text.
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    /// A name local to the sheet at `sheet_index`.
    pub fn local(name: &str, value: &str, sheet_index: u32) -> Self {
        Self {
            local_sheet_id: Some(sheet_index),
            ..Self::new(name, value)
        }
    }

    /// Print area name for a sheet. `ranges` are A1 ranges (`A1:D4`, `B2`,
    /// optional `$`); they are made absolute and sorted like openpyxl does.
    pub fn print_area(sheet_index: u32, sheet_title: &str, ranges: &[&str]) -> Self {
        let mut parsed: Vec<(Option<Bounds>, &str)> =
            ranges.iter().map(|r| (parse_range(r), *r)).collect();
        parsed.sort_by_key(|(bounds, _)| {
            bounds.map(|(c1, r1, c2, r2)| (c1, r1, c2, r2)).unwrap_or((
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
            ))
        });
        let quoted = quote_sheet_name(sheet_title);
        let mut text = String::new();
        for (index, (bounds, raw)) in parsed.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str(&quoted);
            text.push('!');
            match bounds {
                Some((c1, r1, c2, r2)) => {
                    text.push_str(&format!("${}${}", column_letters(*c1), r1));
                    if (c1, r1) != (c2, r2) || raw.contains(':') {
                        text.push_str(&format!(":${}${}", column_letters(*c2), r2));
                    }
                }
                None => text.push_str(raw),
            }
        }
        Self::local(PRINT_AREA_NAME, &text, sheet_index)
    }

    /// Print titles name for a sheet. `rows` is a 1-based inclusive row range
    /// and `cols` a 1-based inclusive column range. Rows come first, like in
    /// openpyxl.
    pub fn print_titles(
        sheet_index: u32,
        sheet_title: &str,
        rows: Option<(u32, u32)>,
        cols: Option<(u32, u32)>,
    ) -> Self {
        let quoted = quote_sheet_name(sheet_title);
        let mut text = String::new();
        if let Some((first, last)) = rows {
            text.push_str(&format!("{quoted}!${first}:${last}"));
        }
        if let Some((first, last)) = cols {
            if !text.is_empty() {
                text.push(',');
            }
            text.push_str(&format!(
                "{quoted}!${}:${}",
                column_letters(first),
                column_letters(last)
            ));
        }
        Self::local(PRINT_TITLES_NAME, &text, sheet_index)
    }

    /// True for the built-in print area name.
    pub fn is_print_area(&self) -> bool {
        &*self.name == PRINT_AREA_NAME
    }

    /// True for the built-in print titles name.
    pub fn is_print_titles(&self) -> bool {
        &*self.name == PRINT_TITLES_NAME
    }

    /// The reserved built-in suffix (`Print_Area`, ...) when the name is a
    /// built-in `_xlnm.` name.
    pub fn reserved(&self) -> Option<&str> {
        const RESERVED: [&str; 7] = [
            "Print_Area",
            "Print_Titles",
            "Criteria",
            "_FilterDatabase",
            "Extract",
            "Consolidate_Area",
            "Sheet_Title",
        ];
        let rest = self.name.strip_prefix("_xlnm.")?;
        RESERVED.into_iter().find(|r| rest.starts_with(r))
    }

    /// Parse the print area value into plain A1 ranges (no sheet, no `$`).
    pub fn print_area_ranges(&self) -> Vec<Box<str>> {
        split_references(&self.value)
            .into_iter()
            .filter_map(|(_, rest)| {
                let plain: String = rest.chars().filter(|c| *c != '$').collect();
                parse_range(&plain).map(|_| plain.into_boxed_str())
            })
            .collect()
    }

    /// Parse the row part of print titles value as a 1-based inclusive range.
    pub fn print_title_rows(&self) -> Option<(u32, u32)> {
        split_references(&self.value)
            .into_iter()
            .find_map(|(_, rest)| {
                let (a, b) = rest.split_once(':')?;
                let (a, b) = (a.trim_start_matches('$'), b.trim_start_matches('$'));
                Some((a.parse().ok()?, b.parse().ok()?))
            })
    }

    /// Parse the column part of print titles value as a 1-based inclusive range.
    pub fn print_title_cols(&self) -> Option<(u32, u32)> {
        split_references(&self.value)
            .into_iter()
            .find_map(|(_, rest)| {
                let (a, b) = rest.split_once(':')?;
                Some((
                    column_number(a.trim_start_matches('$'))?,
                    column_number(b.trim_start_matches('$'))?,
                ))
            })
    }

    /// Sheet title of the first reference of the value (unquoted), if any.
    pub fn sheet_title(&self) -> Option<String> {
        split_references(&self.value)
            .into_iter()
            .find_map(|(sheet, _)| sheet)
    }

    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        let opt = |v: &Option<Box<str>>| v.as_ref().map_or(0, |s| s.len());
        self.name.len()
            + self.value.len()
            + opt(&self.comment)
            + opt(&self.custom_menu)
            + opt(&self.description)
            + opt(&self.help)
            + opt(&self.status_bar)
            + opt(&self.shortcut_key)
    }
}

/// (min_col, min_row, max_col, max_row), 1-based.
type Bounds = (u32, u32, u32, u32);

/// Quote a sheet title like openpyxl `quote_sheetname` (always quoted).
pub fn quote_sheet_name(title: &str) -> String {
    format!("'{}'", title.replace('\'', "''"))
}

/// Column letters for a 1-based column number (`1` -> `A`).
fn column_letters(mut number: u32) -> String {
    let mut out = Vec::new();
    while number > 0 {
        let rem = (number - 1) % 26;
        out.push(b'A' + rem as u8);
        number = (number - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// 1-based column number for letters, `None` when invalid.
fn column_number(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut n = 0u32;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return None;
        }
        n = n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1);
    }
    Some(n)
}

fn parse_cell(text: &str) -> Option<(u32, u32)> {
    let text = text.replace('$', "");
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let col = column_number(&text[..split])?;
    let row: u32 = text[split..].parse().ok()?;
    Some((col, row))
}

/// Parse `A1` or `A1:B2` (optional `$`) into (min_col, min_row, max_col, max_row).
fn parse_range(text: &str) -> Option<Bounds> {
    match text.split_once(':') {
        Some((a, b)) => {
            let (c1, r1) = parse_cell(a)?;
            let (c2, r2) = parse_cell(b)?;
            Some((c1, r1, c2, r2))
        }
        None => {
            let (c, r) = parse_cell(text)?;
            Some((c, r, c, r))
        }
    }
}

/// Split a comma separated reference list into (sheet title, range text).
fn split_references(value: &str) -> Vec<(Option<String>, &str)> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        let mut sheet = None;
        let start;
        if bytes[pos] == b'\'' {
            let mut title = String::new();
            let mut i = pos + 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if bytes.get(i + 1) == Some(&b'\'') {
                        title.push('\'');
                        i += 2;
                        continue;
                    }
                    closed = true;
                    i += 1;
                    break;
                }
                // Copy one UTF-8 character.
                let ch_len = value[i..].chars().next().map_or(1, char::len_utf8);
                title.push_str(&value[i..i + ch_len]);
                i += ch_len;
            }
            if !closed || bytes.get(i) != Some(&b'!') {
                return out;
            }
            sheet = Some(title);
            start = i + 1;
        } else {
            let end = value[pos..].find([',']).map_or(value.len(), |e| pos + e);
            match value[pos..end].find('!') {
                Some(bang) => {
                    sheet = Some(value[pos..pos + bang].to_owned());
                    start = pos + bang + 1;
                }
                None => start = pos,
            }
        }
        let end = value[start..].find(',').map_or(value.len(), |e| start + e);
        out.push((sheet, &value[start..end]));
        pos = end + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn print_area_text_matches_openpyxl() {
        let n = DefinedName::print_area(0, "My Sheet's", &["F6:G8", "A1:D4"]);
        assert_eq!(&*n.value, "'My Sheet''s'!$A$1:$D$4,'My Sheet''s'!$F$6:$G$8");
        assert_eq!(n.local_sheet_id, Some(0));
        assert!(n.is_print_area());
        assert_eq!(
            n.print_area_ranges(),
            vec!["A1:D4".into(), "F6:G8".into()] as Vec<Box<str>>
        );
        assert_eq!(n.sheet_title().as_deref(), Some("My Sheet's"));
        assert_eq!(&*DefinedName::print_area(1, "B", &["C3"]).value, "'B'!$C$3");
    }

    #[test]
    fn print_titles_text_matches_openpyxl() {
        let n = DefinedName::print_titles(0, "My Sheet's", Some((1, 2)), Some((1, 2)));
        assert_eq!(&*n.value, "'My Sheet''s'!$1:$2,'My Sheet''s'!$A:$B");
        assert_eq!(n.print_title_rows(), Some((1, 2)));
        assert_eq!(n.print_title_cols(), Some((1, 2)));
        let n = DefinedName::print_titles(1, "B", Some((3, 3)), None);
        assert_eq!(&*n.value, "'B'!$3:$3");
        assert_eq!(n.print_title_cols(), None);
        assert_eq!(n.reserved(), Some("Print_Titles"));
    }
}
