//! Owned opaque theme data shared by format readers, writers and future bindings.
/// Exact theme-part bytes, including unknown drawing sections and typeface names.
/// This core container does not interpret or validate an XML serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    bytes: Box<[u8]>,
}
impl Theme {
    /// Move an existing owned theme buffer into the canonical representation.
    pub fn from_bytes(bytes: Box<[u8]>) -> Self {
        Self { bytes }
    }
    /// Borrow the original serialization without cloning.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Managed retained payload and container bytes, excluding allocator overhead.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>().saturating_add(self.bytes.len())
    }
}
