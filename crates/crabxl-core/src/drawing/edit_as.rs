// Adapted from umya-spreadsheet 3.1.0, aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b.
// Copyright (c) 2020 MathNya. MIT; see third_party/licenses/umya-spreadsheet-MIT.txt.
use std::str::FromStr;

use crate::{Error, ErrorKind};
/// How a two-cell drawing responds to worksheet resizing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawingEditAs {
    /// Keep position and size fixed.
    Absolute,
    /// Move with the first cell, keeping size fixed.
    OneCell,
    /// Move and resize with both cells.
    TwoCell,
}
impl Default for DrawingEditAs {
    #[inline]
    fn default() -> Self {
        Self::TwoCell
    }
}
impl DrawingEditAs {
    #[inline]
    /// Spreadsheet DrawingML attribute value.
    pub const fn as_str(&self) -> &'static str {
        match &self {
            Self::Absolute => "absolute",
            Self::OneCell => "oneCell",
            Self::TwoCell => "twoCell",
        }
    }
}
impl FromStr for DrawingEditAs {
    type Err = Error;

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "absolute" => Ok(Self::Absolute),
            "oneCell" => Ok(Self::OneCell),
            "twoCell" => Ok(Self::TwoCell),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid drawing editAs value",
            )),
        }
    }
}
