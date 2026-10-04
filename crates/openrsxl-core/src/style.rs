/// A workbook-local index into the shared cell-format table.
///
/// Readers, editable models, and writers use the same identity. Index zero is
/// the default record, not a guarantee that its formatting is General. Styles
/// and format interpretation are M2/M5 work; constructing an ID does not prove
/// that a referenced record exists in a particular workbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleId(u32);
impl StyleId {
    /// Construct a workbook-local format identity.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }
    /// Return the zero-based table index.
    pub const fn get(self) -> u32 {
        self.0
    }
}
