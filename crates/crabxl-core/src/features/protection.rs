// SPDX-License-Identifier: MIT
// Field set and password hash follow openpyxl 3.1.5 (MIT), Copyright (c) 2010-2024 openpyxl.
//! Worksheet and workbook protection.

/// Legacy 16-bit Excel password hash (as hexadecimal text), identical to
/// openpyxl's `hash_password`.
pub fn hash_password(password: &str) -> Box<str> {
    let mut hash: u128 = 0;
    let mut length: u128 = 0;
    for (index, ch) in password.chars().enumerate() {
        let shift = (index + 1) as u32;
        let value = u128::from(ch).checked_shl(shift).unwrap_or(0);
        let rotated = value >> 15;
        hash ^= (value & 0x7fff) | rotated;
        length += 1;
    }
    hash ^= length;
    hash ^= 0xCE4B;
    format!("{hash:X}").into_boxed_str()
}

fn opt_len(value: &Option<Box<str>>) -> usize {
    value.as_ref().map_or(0, |v| v.len())
}

/// Worksheet protection (`<sheetProtection>`). `true` means the action is
/// locked while the sheet is protected, except for the two `select_*` flags.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetProtection {
    /// Sheet protection is active.
    pub sheet: bool,
    /// Objects are locked.
    pub objects: bool,
    /// Scenarios are locked.
    pub scenarios: bool,
    /// Formatting cells is locked.
    pub format_cells: bool,
    /// Formatting columns is locked.
    pub format_columns: bool,
    /// Formatting rows is locked.
    pub format_rows: bool,
    /// Inserting columns is locked.
    pub insert_columns: bool,
    /// Inserting rows is locked.
    pub insert_rows: bool,
    /// Inserting hyperlinks is locked.
    pub insert_hyperlinks: bool,
    /// Deleting columns is locked.
    pub delete_columns: bool,
    /// Deleting rows is locked.
    pub delete_rows: bool,
    /// Selecting locked cells is locked.
    pub select_locked_cells: bool,
    /// Selecting unlocked cells is locked.
    pub select_unlocked_cells: bool,
    /// Sorting is locked.
    pub sort: bool,
    /// Using autofilters is locked.
    pub auto_filter: bool,
    /// Using pivot tables is locked.
    pub pivot_tables: bool,
    /// Legacy password hash (hexadecimal, see [`hash_password`]).
    pub password: Option<Box<str>>,
    /// Modern hash algorithm name, such as `SHA-512`.
    pub algorithm_name: Option<Box<str>>,
    /// Base64 hash value.
    pub hash_value: Option<Box<str>>,
    /// Base64 salt value.
    pub salt_value: Option<Box<str>>,
    /// Hash iteration count.
    pub spin_count: Option<u32>,
}
impl Default for SheetProtection {
    fn default() -> Self {
        Self {
            sheet: false,
            objects: false,
            scenarios: false,
            format_cells: true,
            format_columns: true,
            format_rows: true,
            insert_columns: true,
            insert_rows: true,
            insert_hyperlinks: true,
            delete_columns: true,
            delete_rows: true,
            select_locked_cells: false,
            select_unlocked_cells: false,
            sort: true,
            auto_filter: true,
            pivot_tables: true,
            password: None,
            algorithm_name: None,
            hash_value: None,
            salt_value: None,
            spin_count: None,
        }
    }
}
impl SheetProtection {
    /// Hash and store a password and enable protection (openpyxl `set_password`).
    pub fn set_password(&mut self, password: &str) {
        self.password = Some(hash_password(password));
        self.sheet = true;
    }
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        opt_len(&self.password)
            + opt_len(&self.algorithm_name)
            + opt_len(&self.hash_value)
            + opt_len(&self.salt_value)
    }
}

/// Workbook protection (`<workbookProtection>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbookProtection {
    /// Legacy workbook password hash.
    pub workbook_password: Option<Box<str>>,
    /// Character set of the workbook password.
    pub workbook_password_character_set: Option<Box<str>>,
    /// Legacy revisions password hash.
    pub revisions_password: Option<Box<str>>,
    /// Character set of the revisions password.
    pub revisions_password_character_set: Option<Box<str>>,
    /// Lock workbook structure.
    pub lock_structure: Option<bool>,
    /// Lock workbook windows.
    pub lock_windows: Option<bool>,
    /// Lock revisions.
    pub lock_revision: Option<bool>,
    /// Revisions hash algorithm name.
    pub revisions_algorithm_name: Option<Box<str>>,
    /// Revisions base64 hash value.
    pub revisions_hash_value: Option<Box<str>>,
    /// Revisions base64 salt value.
    pub revisions_salt_value: Option<Box<str>>,
    /// Revisions hash iteration count.
    pub revisions_spin_count: Option<u32>,
    /// Workbook hash algorithm name.
    pub workbook_algorithm_name: Option<Box<str>>,
    /// Workbook base64 hash value.
    pub workbook_hash_value: Option<Box<str>>,
    /// Workbook base64 salt value.
    pub workbook_salt_value: Option<Box<str>>,
    /// Workbook hash iteration count.
    pub workbook_spin_count: Option<u32>,
}
impl WorkbookProtection {
    /// Hash and store the workbook password.
    pub fn set_workbook_password(&mut self, password: &str) {
        self.workbook_password = Some(hash_password(password));
    }
    /// Hash and store the revisions password.
    pub fn set_revisions_password(&mut self, password: &str) {
        self.revisions_password = Some(hash_password(password));
    }
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        [
            &self.workbook_password,
            &self.workbook_password_character_set,
            &self.revisions_password,
            &self.revisions_password_character_set,
            &self.revisions_algorithm_name,
            &self.revisions_hash_value,
            &self.revisions_salt_value,
            &self.workbook_algorithm_name,
            &self.workbook_hash_value,
            &self.workbook_salt_value,
        ]
        .into_iter()
        .map(opt_len)
        .sum()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    #[test]
    fn hashes_match_openpyxl() {
        for (input, expected) in [
            ("", "CE4B"),
            ("a", "CE88"),
            ("secret", "DAA7"),
            ("pässwörd €", "D6FC"),
            ("Password123!", "A1ED"),
            ("xxxxxxxxxxxxxxxxxxxx", "C40F"),
            ("pw", "CF75"),
        ] {
            assert_eq!(&*hash_password(input), expected, "{input}");
        }
    }
    #[test]
    fn set_password_enables() {
        let mut p = SheetProtection::default();
        p.set_password("secret");
        assert!(p.sheet);
        assert_eq!(p.password.as_deref(), Some("DAA7"));
        assert!(p.heap_bytes() >= 4);
    }
}
