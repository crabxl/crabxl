//! Shared bounded declaration capture for explicit scans and opt-in row streams.
use crate::xml::{Frame, Scope, attribute, required_attribute};
use crabxl_core::{CellRange, Error, ErrorKind, Hyperlink, Hyperlinks, Result};
use quick_xml::events::Event;

#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) links: Hyperlinks,
    seen: bool,
    active: bool,
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn unsupported(message: &str) -> Error {
    Error::new(ErrorKind::Unsupported, message)
}
impl Capture {
    pub(crate) fn observe(
        &mut self,
        frame: &Frame<'_>,
        maximum: usize,
        reserve: impl FnMut(usize) -> Result<()>,
    ) -> Result<()> {
        self.observe_with(frame, maximum, reserve, |_, link| Ok(Some(link)))
    }

    /// Visit validated declarations before coverage normalization. Returning None
    /// avoids retaining a second metadata collection during ordered value binding.
    pub(crate) fn observe_with(
        &mut self,
        frame: &Frame<'_>,
        maximum: usize,
        mut reserve: impl FnMut(usize) -> Result<()>,
        mut visit: impl FnMut(CellRange, Hyperlink) -> Result<Option<Hyperlink>>,
    ) -> Result<()> {
        match &frame.event {
            Event::Start(e)
                if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"hyperlinks" =>
            {
                if self.seen || frame.scope != Scope::Spreadsheet {
                    return Err(invalid("Invalid or duplicate hyperlinks container"));
                }
                self.seen = true;
                self.active = true;
                for a in e.attributes() {
                    let a = a.map_err(|_| invalid("Invalid hyperlink container attribute"))?;
                    if a.key.as_namespace_binding().is_none() {
                        return Err(unsupported("Unknown hyperlink container attributes"));
                    }
                }
            }
            Event::Start(e) if self.active => {
                if frame.depth != 3
                    || frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"hyperlink"
                {
                    return Err(unsupported("Unknown hyperlink content"));
                }
                for (index, a) in e.attributes().enumerate() {
                    let a = a.map_err(|_| invalid("Invalid hyperlink attribute"))?;
                    if a.key.as_namespace_binding().is_none()
                        && frame.office_relationship_attribute != Some(index)
                        && !matches!(
                            a.key.as_ref().as_bytes(),
                            b"ref" | b"location" | b"display" | b"tooltip"
                        )
                    {
                        return Err(unsupported("Unknown hyperlink attributes"));
                    }
                }
                let range: CellRange = required_attribute(e, b"ref")?.parse()?;
                let link = Hyperlink {
                    reference: (range.start != range.end)
                        .then(|| range.to_string().into_boxed_str()),
                    location: attribute(e, b"location")?.map(String::into_boxed_str),
                    display: attribute(e, b"display")?.map(String::into_boxed_str),
                    tooltip: attribute(e, b"tooltip")?.map(String::into_boxed_str),
                    relationship_id: frame.office_relationship.as_deref().map(Into::into),
                    ..Default::default()
                };
                if link.heap_bytes().saturating_add(size_of::<Hyperlink>()) > maximum {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Hyperlink declaration exceeds allowance",
                    ));
                }
                if let Some(link) = visit(range, link)? {
                    reserve(self.links.declaration_peak_bytes(range.start, &link)?)?;
                    self.links.set_declaration(range.start, link, maximum)?;
                }
            }
            Event::End(e)
                if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"hyperlinks" =>
            {
                self.active = false
            }
            Event::Text(t)
                if self.active && !t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
            {
                return Err(invalid("Unexpected hyperlink text"));
            }
            Event::CData(_) | Event::GeneralRef(_) | Event::Comment(_) | Event::PI(_)
                if self.active =>
            {
                return Err(unsupported("Unknown hyperlink payload"));
            }
            _ => {}
        }
        Ok(())
    }
}
