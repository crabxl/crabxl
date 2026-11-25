//! Reference updates for row and column insertions and deletions.
//!
//! The rules follow spreadsheet semantics. On insertion, references at or
//! after the insertion point move, and ranges that span the point grow. On
//! deletion, references after the deleted block move back, ranges shrink, and
//! references that lie completely inside the block become `#REF!`. Absolute
//! (`$`) and relative references move alike.

mod apply;

use crate::{CellAddress, CellRange, Error, ErrorKind, MAX_COLUMNS, MAX_ROWS, Result};
use std::fmt::Write;

/// The axis of a structural edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Rows: references change their row numbers.
    Rows,
    /// Columns: references change their column letters.
    Columns,
}

/// An insertion or deletion of whole rows or columns on one sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralEdit {
    /// Rows or columns.
    pub axis: Axis,
    /// Zero-based index of the first inserted or deleted row or column.
    pub at: u32,
    /// Number of rows or columns. At least one.
    pub count: u32,
    /// Insertion (`true`) or deletion (`false`).
    pub insert: bool,
}

/// The result of applying a structural edit to a range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeUpdate {
    /// The edit does not affect the range.
    Unchanged,
    /// The range moved, grew or shrank.
    Moved(CellRange),
    /// The edit deleted every cell of the range.
    Removed,
}

impl StructuralEdit {
    /// Insert `count` rows before the zero-based row `at`.
    pub fn insert_rows(at: u32, count: u32) -> Result<Self> {
        Self::new(Axis::Rows, at, count, true)
    }
    /// Delete `count` rows starting at the zero-based row `at`.
    pub fn delete_rows(at: u32, count: u32) -> Result<Self> {
        Self::new(Axis::Rows, at, count, false)
    }
    /// Insert `count` columns before the zero-based column `at`.
    pub fn insert_columns(at: u32, count: u32) -> Result<Self> {
        Self::new(Axis::Columns, at, count, true)
    }
    /// Delete `count` columns starting at the zero-based column `at`.
    pub fn delete_columns(at: u32, count: u32) -> Result<Self> {
        Self::new(Axis::Columns, at, count, false)
    }
    fn new(axis: Axis, at: u32, count: u32, insert: bool) -> Result<Self> {
        let limit = match axis {
            Axis::Rows => MAX_ROWS,
            Axis::Columns => MAX_COLUMNS,
        };
        if count == 0 || at.checked_add(count).is_none_or(|end| end > limit) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Structural edit lies outside the worksheet",
            ));
        }
        Ok(Self {
            axis,
            at,
            count,
            insert,
        })
    }
    fn limit(&self) -> u32 {
        match self.axis {
            Axis::Rows => MAX_ROWS,
            Axis::Columns => MAX_COLUMNS,
        }
    }
    /// Map one index. `None` means the index was deleted.
    pub fn map_index(&self, index: u32) -> Result<Option<u32>> {
        if self.insert {
            if index < self.at {
                return Ok(Some(index));
            }
            let moved = index + self.count;
            if moved >= self.limit() {
                return Err(beyond_edge());
            }
            Ok(Some(moved))
        } else if index < self.at {
            Ok(Some(index))
        } else if index >= self.at + self.count {
            Ok(Some(index - self.count))
        } else {
            Ok(None)
        }
    }
    /// Map an inclusive span. `None` means every index was deleted. On
    /// insertion, a span that reaches the last row or column keeps its end
    /// there, as a full-height or full-width range does.
    pub fn map_span(&self, start: u32, end: u32) -> Result<Option<(u32, u32)>> {
        let (start, end) = (start.min(end), start.max(end));
        if self.insert {
            if end < self.at {
                return Ok(Some((start, end)));
            }
            let last = self.limit() - 1;
            let new_end = if end == last {
                last
            } else {
                end.checked_add(self.count)
                    .filter(|end| *end <= last)
                    .ok_or_else(beyond_edge)?
            };
            let new_start = if start >= self.at {
                start
                    .checked_add(self.count)
                    .filter(|start| *start <= new_end)
                    .ok_or_else(beyond_edge)?
            } else {
                start
            };
            return Ok(Some((new_start, new_end)));
        }
        let deleted_end = self.at + self.count;
        let new_start = if start < self.at {
            start
        } else if start >= deleted_end {
            start - self.count
        } else {
            self.at
        };
        let new_end = if end < self.at {
            Some(end)
        } else if end >= deleted_end {
            Some(end - self.count)
        } else {
            self.at.checked_sub(1)
        };
        Ok(new_end
            .filter(|end| *end >= new_start)
            .map(|end| (new_start, end)))
    }
    /// Apply the edit to a range on the edited sheet.
    pub fn update_range(&self, range: CellRange) -> Result<RangeUpdate> {
        let (start, end) = match self.axis {
            Axis::Rows => (range.start.row.get(), range.end.row.get()),
            Axis::Columns => (range.start.column.get(), range.end.column.get()),
        };
        let Some((new_start, new_end)) = self.map_span(start, end)? else {
            return Ok(RangeUpdate::Removed);
        };
        if (new_start, new_end) == (start, end) {
            return Ok(RangeUpdate::Unchanged);
        }
        let moved = match self.axis {
            Axis::Rows => CellRange::new(
                CellAddress::new(new_start, range.start.column.get())?,
                CellAddress::new(new_end, range.end.column.get())?,
            )?,
            Axis::Columns => CellRange::new(
                CellAddress::new(range.start.row.get(), new_start)?,
                CellAddress::new(range.end.row.get(), new_end)?,
            )?,
        };
        Ok(RangeUpdate::Moved(moved))
    }
    /// Apply the edit to a list of ranges, such as a `sqref`. Removed ranges
    /// leave the list. Returns `None` when nothing changed.
    pub fn update_ranges(&self, ranges: &[CellRange]) -> Result<Option<Vec<CellRange>>> {
        let mut changed = false;
        let mut output = Vec::with_capacity(ranges.len());
        for range in ranges {
            match self.update_range(*range)? {
                RangeUpdate::Unchanged => output.push(*range),
                RangeUpdate::Moved(moved) => {
                    changed = true;
                    output.push(moved);
                }
                RangeUpdate::Removed => changed = true,
            }
        }
        Ok(changed.then_some(output))
    }
    /// Update the references of a formula, with or without its leading `=`.
    ///
    /// `sheet` is the edited sheet. `host` is the sheet that holds the
    /// formula; its unqualified references belong to it. With `host` set to
    /// `None` (defined names, chart series), an unqualified cell reference is
    /// rejected, because its sheet is unknown. References to other workbooks
    /// and structured table references are left alone. Returns `None` when
    /// nothing changed.
    pub fn update_formula(
        &self,
        formula: &str,
        host: Option<&str>,
        sheet: &str,
    ) -> Result<Option<String>> {
        let (prefix, body) = match formula.strip_prefix('=') {
            Some(body) => ("=", body),
            None => ("", formula),
        };
        if body.is_empty()
            || !body
                .bytes()
                .any(|byte| byte.is_ascii_alphabetic() || byte == b':')
        {
            return Ok(None);
        }
        let source = format!("={body}");
        let tokens = crate::tokenize_formula(&source, source.len().saturating_mul(4).max(4096))?;
        let mut output = String::with_capacity(formula.len() + 8);
        output.push_str(prefix);
        let mut changed = false;
        for token in &tokens {
            let value = token.value.as_ref();
            let updated = match (token.kind, token.subtype) {
                (crate::TokenKind::Operand, crate::TokenSubtype::Range) => {
                    self.update_operand(value, host, sheet)?
                }
                (crate::TokenKind::Function, crate::TokenSubtype::Open) => {
                    // A reference can lead a function call, as in `A1:INDEX(`.
                    match value.rfind(':') {
                        Some(split) if !value[..split].contains('(') => self
                            .update_operand(&value[..split], host, sheet)?
                            .map(|reference| format!("{reference}{}", &value[split..])),
                        _ => None,
                    }
                }
                _ => None,
            };
            match updated {
                Some(text) => {
                    changed = true;
                    output.push_str(&text);
                }
                None => output.push_str(value),
            }
        }
        Ok(changed.then_some(output))
    }
    fn update_operand(
        &self,
        operand: &str,
        host: Option<&str>,
        sheet: &str,
    ) -> Result<Option<String>> {
        if operand.starts_with('[') {
            return Ok(None); // Another workbook.
        }
        let (qualifier, area) = split_qualifier(operand);
        let spill = area.ends_with('#');
        let area = area.strip_suffix('#').unwrap_or(area);
        let Some(parsed) = Area::parse(area) else {
            return Ok(None); // A name, a table reference or an error literal.
        };
        let target = match qualifier {
            None => match host {
                Some(host) => host,
                None => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        format!(
                            "Cannot update the unqualified reference {operand}: its sheet is unknown"
                        ),
                    ));
                }
            },
            Some(qualifier) => {
                let name = unquote(qualifier);
                if name.starts_with('[') {
                    return Ok(None);
                }
                if let Some((first, last)) = name.split_once(':') {
                    if first.eq_ignore_ascii_case(sheet) || last.eq_ignore_ascii_case(sheet) {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            format!("Cannot update the 3-D reference {operand}"),
                        ));
                    }
                    return Ok(None);
                }
                return self.rewrite(operand, qualifier, area, parsed, spill, &name, sheet);
            }
        };
        self.rewrite(operand, "", area, parsed, spill, target, sheet)
    }
    #[allow(clippy::too_many_arguments)]
    fn rewrite(
        &self,
        operand: &str,
        qualifier: &str,
        area: &str,
        parsed: Area,
        spill: bool,
        target: &str,
        sheet: &str,
    ) -> Result<Option<String>> {
        if !target.eq_ignore_ascii_case(sheet) {
            return Ok(None);
        }
        let Some(updated) = parsed.update(self)? else {
            return Ok(None);
        };
        let mut text = String::with_capacity(operand.len() + 4);
        if !qualifier.is_empty() {
            text.push_str(qualifier);
            text.push('!');
        }
        match updated {
            Some(area) => {
                area.write(&mut text);
                if spill {
                    text.push('#');
                }
            }
            None => text.push_str("#REF!"),
        }
        if text == operand || (qualifier.is_empty() && text == area) {
            return Ok(None);
        }
        Ok(Some(text))
    }
}

fn beyond_edge() -> Error {
    Error::new(
        ErrorKind::InvalidData,
        "The edit would move a reference beyond the edge of the worksheet",
    )
}

/// Split `Sheet!A1` into `("Sheet", "A1")`, honoring quoted names.
fn split_qualifier(operand: &str) -> (Option<&str>, &str) {
    let bytes = operand.as_bytes();
    let mut quoted = false;
    let mut split = None;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'\'' => quoted = !quoted,
            b'!' if !quoted => split = Some(index),
            _ => {}
        }
    }
    match split {
        Some(index) => (Some(&operand[..index]), &operand[index + 1..]),
        None => (None, operand),
    }
}

fn unquote(name: &str) -> String {
    match name
        .strip_prefix('\'')
        .and_then(|name| name.strip_suffix('\''))
    {
        Some(inner) => inner.replace("''", "'"),
        None => name.to_owned(),
    }
}

#[derive(Clone, Copy, Debug)]
struct Point {
    column: Option<(u32, bool)>,
    row: Option<(u32, bool)>,
}

impl Point {
    fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let mut at = 0;
        let column_absolute = bytes.first() == Some(&b'$');
        at += usize::from(column_absolute);
        let letters = at;
        let mut column = 0u32;
        while let Some(letter) = bytes.get(at).filter(|byte| byte.is_ascii_alphabetic()) {
            column = column * 26 + u32::from(letter.to_ascii_uppercase() - b'A' + 1);
            at += 1;
            if at - letters > 3 {
                return None;
            }
        }
        let has_column = at > letters;
        if !has_column && column_absolute && bytes.get(at).is_some_and(u8::is_ascii_digit) {
            // `$3` is an absolute row in a row range.
            let digits = &text[at..];
            let row = parse_row(digits)?;
            return Some(Self {
                column: None,
                row: Some((row, true)),
            });
        }
        if has_column && column > MAX_COLUMNS {
            return None;
        }
        let row_absolute = has_column && bytes.get(at) == Some(&b'$');
        at += usize::from(row_absolute);
        let digits = &text[at..];
        let row = if digits.is_empty() {
            if row_absolute {
                return None;
            }
            None
        } else {
            Some((parse_row(digits)?, row_absolute))
        };
        if !has_column && row.is_none() {
            return None;
        }
        Some(Self {
            column: has_column.then(|| (column - 1, column_absolute)),
            row,
        })
    }
    fn write(&self, output: &mut String) {
        if let Some((column, absolute)) = self.column {
            if absolute {
                output.push('$');
            }
            let _ = crate::address::write_column_name(output, column);
        }
        if let Some((row, absolute)) = self.row {
            if absolute {
                output.push('$');
            }
            let _ = write!(output, "{}", row + 1);
        }
    }
}

fn parse_row(digits: &str) -> Option<u32> {
    if digits.is_empty() || digits.len() > 7 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row: u32 = digits.parse().ok()?;
    (1..=MAX_ROWS).contains(&row).then(|| row - 1)
}

/// A cell, a cell range, whole columns or whole rows.
#[derive(Clone, Copy, Debug)]
struct Area {
    start: Point,
    end: Option<Point>,
}

impl Area {
    fn parse(text: &str) -> Option<Self> {
        let (first, second) = match text.split_once(':') {
            Some((first, second)) => (first, Some(second)),
            None => (text, None),
        };
        let start = Point::parse(first)?;
        let end = match second {
            Some(second) => Some(Point::parse(second)?),
            None => None,
        };
        let shape = |point: &Point| (point.column.is_some(), point.row.is_some());
        match end {
            // A single point must be a full cell reference.
            None if shape(&start) != (true, true) => None,
            Some(end) if shape(&start) != shape(&end) => None,
            _ => Some(Self { start, end }),
        }
    }
    /// `Ok(None)`: unchanged. `Ok(Some(None))`: deleted. `Ok(Some(Some(area)))`: moved.
    fn update(&self, edit: &StructuralEdit) -> Result<Option<Option<Self>>> {
        let pick = |point: &Point| match edit.axis {
            Axis::Rows => point.row,
            Axis::Columns => point.column,
        };
        let Some((first, first_absolute)) = pick(&self.start) else {
            return Ok(None); // Whole rows on a column edit, or the reverse.
        };
        let mut updated = *self;
        match self.end {
            None => match edit.map_index(first)? {
                None => return Ok(Some(None)),
                Some(index) if index == first => return Ok(None),
                Some(index) => set(&mut updated.start, edit.axis, index, first_absolute),
            },
            Some(end) => {
                let Some((last, last_absolute)) = pick(&end) else {
                    return Ok(None);
                };
                let (low, high) = (first.min(last), first.max(last));
                match edit.map_span(low, high)? {
                    None => return Ok(Some(None)),
                    Some(span) if span == (low, high) => return Ok(None),
                    Some((new_low, new_high)) => {
                        let (start_index, end_index) = if first <= last {
                            (new_low, new_high)
                        } else {
                            (new_high, new_low)
                        };
                        set(&mut updated.start, edit.axis, start_index, first_absolute);
                        let mut new_end = end;
                        set(&mut new_end, edit.axis, end_index, last_absolute);
                        updated.end = Some(new_end);
                    }
                }
            }
        }
        Ok(Some(Some(updated)))
    }
    fn write(&self, output: &mut String) {
        self.start.write(output);
        if let Some(end) = &self.end {
            output.push(':');
            end.write(output);
        }
    }
}

fn set(point: &mut Point, axis: Axis, index: u32, absolute: bool) {
    match axis {
        Axis::Rows => point.row = Some((index, absolute)),
        Axis::Columns => point.column = Some((index, absolute)),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn update(edit: StructuralEdit, formula: &str) -> String {
        edit.update_formula(formula, Some("Data"), "Data")
            .unwrap()
            .unwrap_or_else(|| formula.to_owned())
    }

    #[test]
    fn insertion_moves_and_grows_references() {
        let edit = StructuralEdit::insert_rows(4, 2).unwrap(); // Before row 5.
        assert_eq!(update(edit, "=A4+A5+$B$10"), "=A4+A7+$B$12");
        assert_eq!(update(edit, "=SUM(A1:A10)"), "=SUM(A1:A12)");
        assert_eq!(update(edit, "=SUM(A5:A10)"), "=SUM(A7:A12)");
        assert_eq!(update(edit, "=SUM(A1:A4)"), "=SUM(A1:A4)");
        assert_eq!(update(edit, "=SUM(B:B)+SUM(3:8)"), "=SUM(B:B)+SUM(3:10)");
        assert_eq!(update(edit, "=SUM(A1:A1048576)"), "=SUM(A1:A1048576)");
        assert_eq!(
            update(edit, "=Data!A9+'Data'!A9+Other!A9"),
            "=Data!A11+'Data'!A11+Other!A9"
        );
        assert_eq!(
            update(edit, "=A9#+[1]Data!A9+Table1[Col]+TaxRate"),
            "=A11#+[1]Data!A9+Table1[Col]+TaxRate"
        );
        assert_eq!(update(edit, "=A9:INDEX(B:B,2)"), "=A11:INDEX(B:B,2)");
        let columns = StructuralEdit::insert_columns(1, 1).unwrap(); // Before column B.
        assert_eq!(
            update(columns, "=A1+B1+$C$2+SUM(A:C)+SUM(1:1)"),
            "=A1+C1+$D$2+SUM(A:D)+SUM(1:1)"
        );
    }

    #[test]
    fn deletion_shrinks_and_invalidates_references() {
        let edit = StructuralEdit::delete_rows(4, 2).unwrap(); // Rows 5 and 6.
        assert_eq!(update(edit, "=A4+A7"), "=A4+A5");
        assert_eq!(update(edit, "=A5+Data!A6"), "=#REF!+Data!#REF!");
        assert_eq!(update(edit, "=SUM(A1:A10)"), "=SUM(A1:A8)");
        assert_eq!(update(edit, "=SUM(A5:A6)"), "=SUM(#REF!)");
        assert_eq!(update(edit, "=SUM(A6:A9)"), "=SUM(A5:A7)");
        assert_eq!(update(edit, "=SUM(5:6)+SUM(A:A)"), "=SUM(#REF!)+SUM(A:A)");
    }

    #[test]
    fn other_sheets_and_unknown_sheets() {
        let edit = StructuralEdit::insert_rows(0, 1).unwrap();
        assert_eq!(
            edit.update_formula("=A1", Some("Other"), "Data").unwrap(),
            None
        );
        assert_eq!(
            edit.update_formula("='O''Brien'!A1", Some("Other"), "O'Brien")
                .unwrap()
                .unwrap(),
            "='O''Brien'!A2"
        );
        assert_eq!(
            edit.update_formula("Data!$A$1:$A$3", None, "Data")
                .unwrap()
                .unwrap(),
            "Data!$A$2:$A$4"
        );
        assert_eq!(
            edit.update_formula("=A1", None, "Data").unwrap_err().kind(),
            ErrorKind::Unsupported
        );
        assert_eq!(
            edit.update_formula("=SUM(Data:Other!A1)", Some("X"), "Data")
                .unwrap_err()
                .kind(),
            ErrorKind::Unsupported
        );
        assert_eq!(
            edit.update_formula("=1+\"A1\"", Some("Data"), "Data")
                .unwrap(),
            None
        );
    }

    #[test]
    fn ranges_and_edges() {
        let range = |text: &str| text.parse::<CellRange>().unwrap();
        let delete = StructuralEdit::delete_columns(1, 2).unwrap(); // B and C.
        assert_eq!(
            delete.update_range(range("B2:C9")).unwrap(),
            RangeUpdate::Removed
        );
        assert_eq!(
            delete.update_range(range("A1:D1")).unwrap(),
            RangeUpdate::Moved(range("A1:B1"))
        );
        assert_eq!(
            delete.update_range(range("A1:A9")).unwrap(),
            RangeUpdate::Unchanged
        );
        let insert = StructuralEdit::insert_rows(0, 1).unwrap();
        assert!(
            insert
                .update_formula("=A1048576", Some("Data"), "Data")
                .is_err()
        );
        assert!(StructuralEdit::insert_rows(MAX_ROWS, 1).is_err());
        assert_eq!(
            insert
                .update_ranges(&[range("A1:A2"), range("C3")])
                .unwrap()
                .unwrap(),
            vec![range("A2:A3"), range("C4")]
        );
    }
}
