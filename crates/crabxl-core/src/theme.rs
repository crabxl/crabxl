//! Owned opaque theme data shared by format readers, writers and future bindings.
use std::sync::Arc;
/// Exact theme-part bytes, including unknown drawing sections and typeface names.
/// This core container does not interpret or validate an XML serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    bytes: Arc<[u8]>,
}
impl Theme {
    /// Transfer a theme buffer into immutable shared ownership.
    /// Conversion may transiently retain the input buffer while allocating the shared block.
    pub fn from_bytes(bytes: Box<[u8]>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }
    /// Borrow the original serialization without cloning.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Conservative full payload/container/reference-count charge per holder.
    /// Clones share bytes without copying; allocator overhead is additional.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(2 * size_of::<usize>())
            .saturating_add(self.bytes.len())
    }
}
