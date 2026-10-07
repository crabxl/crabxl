//! Borrowed output identities for caller-held metadata views.
use crabxl_core::{CellAddress, CellRange, Hyperlink, Hyperlinks, Result, SheetId};

/// A declaration and its planned worksheet-local output relationship identity.
/// The projection borrows canonical payloads and never expands covered cells.
pub struct HyperlinkOutput<'a> {
    /// Stable workbook-local worksheet identity.
    pub sheet: SheetId,
    /// Canonical declaration owner, independent of its serialized reference.
    pub owner: CellAddress,
    /// Actual range coverage when the declaration was adopted as a rectangle.
    pub coverage: Option<CellRange>,
    /// Canonical properties; source relationship hints remain unchanged.
    pub link: &'a Hyperlink,
    /// Planned output identity; location-only declarations have no relationship.
    pub identity: Option<&'a str>,
}

/// Visit ordinary owned-package identities in the exact shared writer order.
/// The callback may retain selected identifiers; no persistent table is built.
pub fn visit_owned_hyperlink_ids(
    sheet: SheetId,
    links: &Hyperlinks,
    mut visit: impl FnMut(HyperlinkOutput<'_>) -> Result<()>,
) -> Result<()> {
    super::validate(links)?;
    for (index, (owner, link)) in links.iter().enumerate() {
        let identity = link.target.as_ref().map(|_| format!("rId{}", index + 1));
        visit(HyperlinkOutput {
            sheet,
            owner,
            coverage: links.covering_range(owner),
            link,
            identity: identity.as_deref(),
        })?;
    }
    Ok(())
}
