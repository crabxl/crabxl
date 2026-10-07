// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Metadata operations.
use super::*;

impl<'a, R: Read + Seek> Rows<'a, R> {
    /// Retain the explicit metadata of the most recently decoded row.
    /// Disabled by default so scalar streaming avoids dimension parsing.
    pub fn capture_dimensions(&mut self) {
        self.capture_dimensions = true;
    }
    /// Borrow metadata for the most recently decoded row, when capture is enabled.
    pub fn row_dimension(&self) -> Option<&crabxl_core::RowDimension> {
        self.row_dimension.as_ref()
    }
}
