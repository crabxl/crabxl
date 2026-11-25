//! Rows of values: conversion to and from Rust types, and dense grids.

use crate::value::{CellInput, FromValue, IntoCell, Value};
use crate::{Error, ErrorKind, Result};
use std::ops::Index;

/// Converts into the cells of one row. Implemented for tuples of up to 16
/// [`IntoCell`] values, arrays, `Vec` and slices.
pub trait IntoRow {
    /// Convert the row.
    fn into_cells(self) -> Vec<CellInput>;
}

impl<T: IntoCell, const N: usize> IntoRow for [T; N] {
    fn into_cells(self) -> Vec<CellInput> {
        self.into_iter().map(IntoCell::into_cell).collect()
    }
}
impl<T: IntoCell> IntoRow for Vec<T> {
    fn into_cells(self) -> Vec<CellInput> {
        self.into_iter().map(IntoCell::into_cell).collect()
    }
}
impl<T: IntoCell + Clone> IntoRow for &[T] {
    fn into_cells(self) -> Vec<CellInput> {
        self.iter().cloned().map(IntoCell::into_cell).collect()
    }
}
impl<T: IntoCell + Clone> IntoRow for &Vec<T> {
    fn into_cells(self) -> Vec<CellInput> {
        self.as_slice().into_cells()
    }
}

/// Converts from the values of one row. Implemented for tuples of up to 16
/// [`FromValue`] types and for `Vec<T>`. Missing trailing cells read as
/// blank. Errors name the zero-based position in the row.
pub trait FromRow: Sized {
    /// Convert the row.
    fn from_row(values: &[Value]) -> Result<Self>;
}

impl<T: FromValue> FromRow for Vec<T> {
    fn from_row(values: &[Value]) -> Result<Self> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| T::from_value(value).map_err(|error| at(index, error)))
            .collect()
    }
}

fn at(index: usize, error: Error) -> Error {
    Error::new(
        error.kind(),
        format!("Position {index}: {}", error.message()),
    )
}

static BLANK: Value = Value::Blank;

macro_rules! tuple_rows {
    ($( ($($name:ident $index:tt),+) )+) => {$(
        impl<$($name: IntoCell),+> IntoRow for ($($name,)+) {
            fn into_cells(self) -> Vec<CellInput> {
                vec![$(self.$index.into_cell()),+]
            }
        }
        impl<$($name: FromValue),+> FromRow for ($($name,)+) {
            fn from_row(values: &[Value]) -> Result<Self> {
                Ok(($(
                    $name::from_value(values.get($index).unwrap_or(&BLANK))
                        .map_err(|error| at($index, error))?,
                )+))
            }
        }
    )+};
}
tuple_rows! {
    (A 0)
    (A 0, B 1)
    (A 0, B 1, C 2)
    (A 0, B 1, C 2, D 3)
    (A 0, B 1, C 2, D 3, E 4)
    (A 0, B 1, C 2, D 3, E 4, F 5)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13, O 14)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13, O 14, P 15)
}

/// A dense rectangle of values, stored row by row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    width: usize,
    values: Vec<Value>,
}

impl Grid {
    /// A grid from its rows. Every row must have the same length.
    pub fn from_rows<R: IntoIterator<Item = Vec<Value>>>(rows: R) -> Result<Self> {
        let mut grid = Self::default();
        for (index, row) in rows.into_iter().enumerate() {
            if index == 0 {
                grid.width = row.len();
            } else if row.len() != grid.width {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Row {index} has {} values; the grid is {} wide",
                        row.len(),
                        grid.width
                    ),
                ));
            }
            grid.values.extend(row);
        }
        Ok(grid)
    }
    pub(crate) fn with_size(width: usize, height: usize) -> Self {
        Self {
            width,
            values: vec![Value::Blank; width * height],
        }
    }
    pub(crate) fn set(&mut self, row: usize, column: usize, value: Value) {
        self.values[row * self.width + column] = value;
    }
    /// The number of columns.
    pub fn width(&self) -> usize {
        self.width
    }
    /// The number of rows.
    pub fn height(&self) -> usize {
        self.values.len().checked_div(self.width).unwrap_or(0)
    }
    /// One row, or `None` past the end.
    pub fn row(&self, index: usize) -> Option<&[Value]> {
        self.values
            .get(index * self.width..(index + 1) * self.width)
    }
    /// The rows in order.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[Value]> {
        self.values.chunks(self.width.max(1))
    }
    /// One value, or `None` outside the grid.
    pub fn get(&self, row: usize, column: usize) -> Option<&Value> {
        (column < self.width)
            .then(|| self.values.get(row * self.width + column))
            .flatten()
    }
    /// Convert every row.
    pub fn rows_as<T: FromRow>(&self) -> Result<Vec<T>> {
        self.rows()
            .enumerate()
            .map(|(index, row)| {
                T::from_row(row).map_err(|error| {
                    Error::new(error.kind(), format!("Row {index}: {}", error.message()))
                })
            })
            .collect()
    }
}

impl Index<(usize, usize)> for Grid {
    type Output = Value;
    fn index(&self, (row, column): (usize, usize)) -> &Value {
        self.get(row, column).unwrap_or(&BLANK)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn tuples_convert_both_ways() {
        let cells = ("North", 12.5, 3, true).into_cells();
        assert_eq!(cells.len(), 4);
        let values = [Value::from("North"), Value::from(12.5)];
        let (name, amount, missing): (String, f64, Option<i64>) =
            FromRow::from_row(&values).unwrap();
        assert_eq!((name.as_str(), amount, missing), ("North", 12.5, None));
        let error = <(f64,)>::from_row(&[Value::from("x")]).unwrap_err();
        assert!(error.message().starts_with("Position 0"));
        let grid = Grid::from_rows(vec![
            vec![Value::from(1), Value::from(2)],
            vec![Value::from(3), Value::from(4)],
        ])
        .unwrap();
        assert_eq!((grid.width(), grid.height()), (2, 2));
        assert_eq!(grid[(1, 0)], Value::from(3));
        let rows: Vec<(i64, i64)> = grid.rows_as().unwrap();
        assert_eq!(rows, [(1, 2), (3, 4)]);
        assert!(Grid::from_rows(vec![vec![Value::Blank], vec![]]).is_err());
    }
}
