//! Schedule one hyperlink replacement at its worksheet schema position.
use super::*;

pub(crate) struct Rewrite<'a> {
    links: &'a Hyperlinks,
    plan: &'a source::Plan,
    written: bool,
}
impl<'a> Rewrite<'a> {
    pub(crate) fn new(links: &'a Hyperlinks, plan: &'a source::Plan) -> Self {
        Self {
            links,
            plan,
            written: false,
        }
    }
    pub(crate) fn before_start(
        &mut self,
        output: &mut impl Write,
        name: &[u8],
        depth: usize,
        uri: Option<&str>,
    ) -> io::Result<bool> {
        if depth != 2 {
            return Ok(false);
        }
        let late = matches!(
            name,
            b"hyperlinks"
                | b"printOptions"
                | b"pageMargins"
                | b"pageSetup"
                | b"headerFooter"
                | b"rowBreaks"
                | b"colBreaks"
                | b"customProperties"
                | b"cellWatches"
                | b"ignoredErrors"
                | b"smartTags"
                | b"drawing"
                | b"legacyDrawing"
                | b"legacyDrawingHF"
                | b"picture"
                | b"oleObjects"
                | b"controls"
                | b"webPublishItems"
                | b"tableParts"
                | b"extLst"
        );
        if late {
            self.write(output, uri)?;
        }
        Ok(name == b"hyperlinks")
    }
    pub(crate) fn before_end(
        &mut self,
        output: &mut impl Write,
        depth: usize,
        uri: Option<&str>,
    ) -> io::Result<()> {
        if depth == 0 {
            self.write(output, uri)?;
        }
        Ok(())
    }
    fn write(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.written {
            self.plan.write_links(output, self.links, uri)?;
            self.written = true;
        }
        Ok(())
    }
}
