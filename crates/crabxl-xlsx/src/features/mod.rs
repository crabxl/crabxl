//! XML codecs for worksheet and workbook features (tables, filters,
//! conditional formats, validation, comments, protection, header/footer,
//! defined names and document properties).
//!
//! Inline worksheet elements are read with `read_*(xml, start, depth)`: the
//! caller has consumed the `Start` event of the element at `depth`; the
//! function consumes events up to and including the matching `End`.
//! Writers emit one complete element. `uri` is the SpreadsheetML namespace of
//! the enclosing document when it differs from the default prefix-free
//! namespace; pass it to `crate::printing::namespace`-style helpers so the
//! element binds correctly inside a prefixed source document.

pub(crate) mod auto_filter;
pub mod bridge;
pub(crate) mod comment;
pub(crate) mod conditional;
pub(crate) mod defined_name;
pub(crate) mod header_footer;
pub(crate) mod package;
pub(crate) mod properties;
pub(crate) mod protection;
pub(crate) mod sheet;
pub(crate) mod table;
pub(crate) mod validation;

use crabxl_core::{Error, ErrorKind};

pub(crate) fn unsupported(message: &str) -> Error {
    Error::new(ErrorKind::Unsupported, message)
}
pub(crate) fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

/// Re-serialize an element and its subtree as raw XML. The `Start` of the
/// element at `depth` was just consumed. Prefixes are kept as written.
pub(crate) fn read_raw<B: std::io::BufRead>(
    xml: &mut crate::xml::XmlStream<B>,
    start: &quick_xml::events::BytesStart<'_>,
    depth: usize,
) -> crabxl_core::Result<Box<str>> {
    use quick_xml::events::Event;
    let mut writer = quick_xml::Writer::new(Vec::new());
    let error = |_| invalid("Cannot copy raw XML");
    writer
        .write_event(Event::Start(start.borrow()))
        .map_err(error)?;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::End(e) if frame.depth + 1 == depth => {
                writer.write_event(Event::End(e)).map_err(error)?;
                break;
            }
            Event::Eof => return Err(invalid("Incomplete raw XML element")),
            event => writer.write_event(event).map_err(error)?,
        }
    }
    String::from_utf8(writer.into_inner())
        .map(String::into_boxed_str)
        .map_err(|_| invalid("Raw XML is not UTF-8"))
}

/// Relationship and content types used by feature parts.
pub(crate) const REL_TABLE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
pub(crate) const REL_COMMENTS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
pub(crate) const REL_VML: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
pub(crate) const CT_TABLE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
pub(crate) const CT_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
pub(crate) const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";

/// Child order of `CT_Worksheet` (ECMA-376 Part 1, 18.3.1.99).
pub(crate) const WORKSHEET_ORDER: &[&[u8]] = &[
    b"sheetPr",
    b"dimension",
    b"sheetViews",
    b"sheetFormatPr",
    b"cols",
    b"sheetData",
    b"sheetCalcPr",
    b"sheetProtection",
    b"protectedRanges",
    b"scenarios",
    b"autoFilter",
    b"sortState",
    b"dataConsolidate",
    b"customSheetViews",
    b"mergeCells",
    b"phoneticPr",
    b"conditionalFormatting",
    b"dataValidations",
    b"hyperlinks",
    b"printOptions",
    b"pageMargins",
    b"pageSetup",
    b"headerFooter",
    b"rowBreaks",
    b"colBreaks",
    b"customProperties",
    b"cellWatches",
    b"ignoredErrors",
    b"smartTags",
    b"drawing",
    b"legacyDrawing",
    b"legacyDrawingHF",
    b"picture",
    b"oleObjects",
    b"controls",
    b"webPublishItems",
    b"tableParts",
    b"extLst",
];

/// Position of a worksheet child element in `WORKSHEET_ORDER`.
pub(crate) fn worksheet_rank(name: &[u8]) -> Option<usize> {
    WORKSHEET_ORDER.iter().position(|item| *item == name)
}
