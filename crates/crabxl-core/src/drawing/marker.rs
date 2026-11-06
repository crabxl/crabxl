// Adapted from umya-spreadsheet 3.1.0, aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b.
// Copyright (c) 2020 MathNya. MIT; see third_party/licenses/umya-spreadsheet-MIT.txt.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
/// Zero-based drawing position with offsets in English Metric Units (914400 per inch).
pub struct DrawingMarker {
    /// Column index.
    pub col: u32,
    /// Horizontal offset in EMUs.
    pub col_off: i32,
    /// Row index.
    pub row: u32,
    /// Vertical offset in EMUs.
    pub row_off: i32,
}
impl DrawingMarker {
    /// Construct a marker at the upper-left corner of a cell.
    pub fn new(row: u32, col: u32) -> crate::Result<Self> {
        crate::CellAddress::new(row, col)?;
        Ok(Self {
            row,
            col,
            ..Self::default()
        })
    }
    /// Validate that the cell lies inside an XLSX worksheet.
    pub fn validate(&self) -> crate::Result<()> {
        crate::CellAddress::new(self.row, self.col).map(|_| ())
    }
}
