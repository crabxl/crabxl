//! Hyperlink operations under the canonical worksheet allowance.
use super::*;

impl Worksheet {
    pub(crate) fn guard_merge_hyperlinks(&self, range: CellRange) -> Result<()> {
        if self.hyperlinks.non_anchor_intersects(range) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Merging hyperlink-bearing non-anchor cells remains unimplemented",
            )
            .with_cell(range.start));
        }
        Ok(())
    }
    /// Borrow sparse links without copying targets or expanding coordinates.
    pub fn hyperlinks(&self) -> &crate::Hyperlinks {
        &self.hyperlinks
    }
    /// Adopt decoded declarations without changing physical values.
    pub fn set_hyperlinks(&mut self, links: crate::Hyperlinks) -> Result<()> {
        let bytes = self
            .charged
            .saturating_sub(self.hyperlinks.heap_bytes())
            .saturating_add(links.heap_bytes());
        self.check(bytes, self.len())?;
        self.hyperlinks = links;
        self.charged = bytes;
        self.dirty = true;
        Ok(())
    }
    /// Replace one link and fill an empty anchor from target/location atomically.
    /// Removing a link leaves its cell value and logical append extent intact.
    pub fn set_hyperlink(
        &mut self,
        address: CellAddress,
        value: Option<crate::Hyperlink>,
    ) -> Result<()> {
        if value.is_some() && self.merges.virtual_style(address).is_some() {
            return Err(Error::new(
                ErrorKind::InvalidState,
                "Merged non-anchor hyperlinks are read-only",
            )
            .with_cell(address));
        }
        let old = self.hyperlinks.heap_bytes();
        let new = self.hyperlinks.proposed_bytes(address, value.as_ref());
        let other = self.charged.saturating_sub(old);
        self.check(other.saturating_add(new), self.len())?;
        if let Some(link) = &value {
            let current = self.get(address);
            if current.is_none_or(|cell| matches!(cell.value, CellValue::Empty)) {
                let cell = Cell {
                    address,
                    style: self.style_at(address),
                    value: link
                        .target
                        .as_ref()
                        .or(link.location.as_ref())
                        .map_or(CellValue::Empty, |text| CellValue::text(text.clone())),
                };
                let maximum = self.limits.max_bytes;
                self.limits.max_bytes = maximum.saturating_sub(new.saturating_sub(old));
                let result = self.set(cell);
                self.limits.max_bytes = maximum;
                result?;
            }
        }
        self.hyperlinks
            .set(address, value, self.limits.max_bytes.saturating_sub(other))?;
        self.charged = self.charged.saturating_sub(old).saturating_add(new);
        self.dirty = true;
        Ok(())
    }
}
