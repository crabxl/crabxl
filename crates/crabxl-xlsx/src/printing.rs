// SPDX-License-Identifier: MIT
// Page printing/break layouts adapted from rust_xlsxwriter worksheet.rs.
// Copyright 2022-2026 John McNamara. See third_party/ports.json.
use crate::{
    encode::{validate_xml_text, write_attribute},
    metadata::{attributes, boolean, integer},
    xml::{Scope, XmlStream},
};
use crabxl_core::{
    Error, ErrorKind, PageBreak, PageOrder, PageOrientation, PaperDimension, PrintOptions,
    PrintSettings, PrintedComments, PrintedErrors, Result,
};
use quick_xml::events::Event;
use std::io::{self, BufRead, Write};
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "Printing extension requires typed support; unchanged original-package preservation remains available",
    )
}

const SHEET_PROPERTIES: &[&str] = &[
    "syncHorizontal",
    "syncVertical",
    "syncRef",
    "transitionEvaluation",
    "transitionEntry",
    "published",
    "codeName",
    "filterMode",
    "enableFormatConditionsCalculation",
];
const TAB_COLOR: &[&str] = &["auto", "indexed", "rgb", "theme", "tint"];
const OUTLINE: &[&str] = &[
    "applyStyles",
    "summaryBelow",
    "summaryRight",
    "showOutlineSymbols",
];
const SHEET_FORMAT: &[&str] = &[
    "baseColWidth",
    "defaultColWidth",
    "defaultRowHeight",
    "customHeight",
    "zeroHeight",
    "thickTop",
    "thickBottom",
    "outlineLevelRow",
    "outlineLevelCol",
];

pub(crate) fn validate(settings: &PrintSettings) -> Result<()> {
    settings.validate()?;
    let properties = settings.properties.as_ref();
    for (attributes, allowed) in [
        (properties.map(|p| &p.attributes), SHEET_PROPERTIES),
        (properties.and_then(|p| p.tab_color.as_ref()), TAB_COLOR),
        (properties.and_then(|p| p.outline.as_ref()), OUTLINE),
        (settings.format.as_ref(), SHEET_FORMAT),
    ] {
        for (name, value) in attributes.into_iter().flatten() {
            if !allowed.contains(&name.as_ref()) {
                return Err(invalid("Unknown sheet property attribute"));
            }
            validate_xml_text(value)?;
        }
    }
    validate_setup(&settings.setup)
}

/// Keep the schema attributes of one element; other unprefixed names are unsupported.
// shortcut: prefixed extension attributes such as x14ac:dyDescent (a rendering hint) are dropped, keep them if an extension needs them.
fn schema_attributes(
    e: &quick_xml::events::BytesStart<'_>,
    allowed: &[&str],
) -> Result<crabxl_core::ElementAttributes> {
    let mut result = Vec::new();
    attributes(e, |name, value| {
        if name.contains(&b':') {
            return Ok(());
        }
        let name = std::str::from_utf8(name).map_err(|_| invalid("Invalid attribute name"))?;
        if !allowed.contains(&name) {
            return Err(unsupported());
        }
        result.push((name.into(), value.into()));
        Ok(())
    })?;
    Ok(result)
}

fn write_element(
    output: &mut impl Write,
    name: &str,
    attributes: &crabxl_core::ElementAttributes,
    uri: Option<&str>,
) -> io::Result<()> {
    write!(output, "<{name}")?;
    namespace(output, uri)?;
    for (name, value) in attributes {
        write_attribute(output, name, value)?;
    }
    output.write_all(b"/>")
}

/// Write `<sheetFormatPr>`, using the default row height when it is not modeled.
pub(crate) fn write_format(
    output: &mut impl Write,
    settings: Option<&PrintSettings>,
    uri: Option<&str>,
) -> io::Result<()> {
    match settings.and_then(|settings| settings.format.as_ref()) {
        Some(attributes) => write_element(output, "sheetFormatPr", attributes, uri),
        None => {
            output.write_all(b"<sheetFormatPr")?;
            namespace(output, uri)?;
            output.write_all(b" baseColWidth=\"8\" defaultRowHeight=\"15\"/>")
        }
    }
}
pub(crate) fn validate_change(change: &crabxl_core::PrintSettingsChange) -> Result<()> {
    change.validate()?;
    if let crabxl_core::PrintSettingsChange::Setup(setup) = change {
        validate_setup(setup)?;
    }
    Ok(())
}
fn validate_setup(setup: &crabxl_core::PageSetup) -> Result<()> {
    for value in setup.paper_height.iter().chain(setup.paper_width.iter()) {
        validate_xml_text(value.as_str())?;
    }
    if let Some(id) = &setup.printer_relationship {
        validate_xml_text(id)?;
    }
    Ok(())
}
pub(crate) fn namespace(output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
    if let Some(uri) = uri {
        write_attribute(output, "xmlns", uri)?;
    }
    Ok(())
}
/// Write the complete `<sheetPr>` element, or nothing when it has no content.
pub(crate) fn write_properties(
    output: &mut impl Write,
    settings: &PrintSettings,
    uri: Option<&str>,
) -> io::Result<()> {
    let page = settings.auto_page_breaks.is_some() || settings.fit_to_page.is_some();
    let properties = settings.properties.as_ref().filter(|p| !p.is_empty());
    if !page && properties.is_none() {
        return Ok(());
    }
    output.write_all(b"<sheetPr")?;
    namespace(output, uri)?;
    for (name, value) in properties.iter().flat_map(|p| &p.attributes) {
        write_attribute(output, name, value)?;
    }
    output.write_all(b">")?;
    if let Some(attributes) = properties.and_then(|p| p.tab_color.as_ref()) {
        write_element(output, "tabColor", attributes, uri)?;
    }
    if let Some(attributes) = properties.and_then(|p| p.outline.as_ref()) {
        write_element(output, "outlinePr", attributes, uri)?;
    }
    write_page_properties(output, settings, uri)?;
    output.write_all(b"</sheetPr>")
}

/// Write only the `<pageSetUpPr>` child of `<sheetPr>`.
fn write_page_properties(
    output: &mut impl Write,
    settings: &PrintSettings,
    uri: Option<&str>,
) -> io::Result<()> {
    if settings.auto_page_breaks.is_none() && settings.fit_to_page.is_none() {
        return Ok(());
    }
    output.write_all(b"<pageSetUpPr")?;
    namespace(output, uri)?;
    for (name, value) in [
        ("autoPageBreaks", settings.auto_page_breaks),
        ("fitToPage", settings.fit_to_page),
    ] {
        if let Some(value) = value {
            write_attribute(output, name, if value { "1" } else { "0" })?;
        }
    }
    output.write_all(b"/>")
}
pub(crate) fn write_page(
    output: &mut impl Write,
    settings: &PrintSettings,
    uri: Option<&str>,
) -> io::Result<()> {
    let options = settings.options;
    if options != PrintOptions::default() {
        output.write_all(b"<printOptions")?;
        namespace(output, uri)?;
        for (name, value) in [
            ("horizontalCentered", options.horizontal_centered),
            ("verticalCentered", options.vertical_centered),
            ("headings", options.headings),
            ("gridLines", options.grid_lines),
            ("gridLinesSet", options.grid_lines_set),
        ] {
            if let Some(value) = value {
                write_attribute(output, name, if value { "1" } else { "0" })?;
            }
        }
        output.write_all(b"/>")?;
    }
    if let Some(margins) = settings.margins {
        output.write_all(b"<pageMargins")?;
        namespace(output, uri)?;
        for (name, value) in [
            ("left", margins.left),
            ("right", margins.right),
            ("top", margins.top),
            ("bottom", margins.bottom),
            ("header", margins.header),
            ("footer", margins.footer),
        ] {
            write!(output, " {name}=\"{value}\"")?;
        }
        output.write_all(b"/>")?;
    }
    let setup = &settings.setup;
    if setup != &crabxl_core::PageSetup::default() {
        output.write_all(b"<pageSetup")?;
        namespace(output, uri)?;
        for (name, value) in [
            ("paperSize", setup.paper_size),
            ("scale", setup.scale),
            ("fitToHeight", setup.fit_to_height),
            ("fitToWidth", setup.fit_to_width),
            ("firstPageNumber", setup.first_page_number),
            ("horizontalDpi", setup.horizontal_dpi),
            ("verticalDpi", setup.vertical_dpi),
            ("copies", setup.copies),
        ] {
            if let Some(value) = value {
                write!(output, " {name}=\"{value}\"")?;
            }
        }
        for (name, value) in [
            ("useFirstPageNumber", setup.use_first_page_number),
            ("usePrinterDefaults", setup.use_printer_defaults),
            ("blackAndWhite", setup.black_and_white),
            ("draft", setup.draft),
        ] {
            if let Some(value) = value {
                write_attribute(output, name, if value { "1" } else { "0" })?;
            }
        }
        for (name, value) in [
            (
                "orientation",
                setup.orientation.map(PageOrientation::as_str),
            ),
            ("pageOrder", setup.page_order.map(PageOrder::as_str)),
            (
                "cellComments",
                setup.cell_comments.map(PrintedComments::as_str),
            ),
            ("errors", setup.errors.map(PrintedErrors::as_str)),
            (
                "paperHeight",
                setup.paper_height.as_ref().map(PaperDimension::as_str),
            ),
            (
                "paperWidth",
                setup.paper_width.as_ref().map(PaperDimension::as_str),
            ),
        ] {
            if let Some(value) = value {
                write_attribute(output, name, value)?;
            }
        }
        if let Some(id) = &setup.printer_relationship {
            write_attribute(
                output,
                "xmlns:r",
                if uri == Some(crate::xml::STRICT_MAIN_URI) {
                    crate::xml::STRICT_OFFICE_REL_URI
                } else {
                    crate::xml::OFFICE_REL_URI
                },
            )?;
            write_attribute(output, "r:id", id)?;
        }
        output.write_all(b"/>")?;
    }
    Ok(())
}
pub(crate) fn write_breaks(
    output: &mut impl Write,
    name: &str,
    breaks: &[PageBreak],
    uri: Option<&str>,
) -> io::Result<()> {
    if breaks.is_empty() {
        return Ok(());
    }
    write!(output, "<{name}")?;
    namespace(output, uri)?;
    // The public descriptor reports all records as manualBreakCount, including false flags.
    write!(
        output,
        " count=\"{}\" manualBreakCount=\"{}\">",
        breaks.len(),
        breaks.len()
    )?;
    for entry in breaks {
        output.write_all(b"<brk")?;
        for (name, value) in [
            ("id", entry.id),
            ("min", entry.minimum),
            ("max", entry.maximum),
        ] {
            if let Some(value) = value {
                write!(output, " {name}=\"{value}\"")?;
            }
        }
        for (name, value) in [("man", entry.manual), ("pt", entry.pivot)] {
            if let Some(value) = value {
                write_attribute(output, name, if value { "1" } else { "0" })?;
            }
        }
        output.write_all(b"/>")?;
    }
    write!(output, "</{name}>")
}

pub(crate) fn read<B: BufRead>(xml: &mut XmlStream<B>, maximum: usize) -> Result<PrintSettings> {
    let part = xml.part().to_owned();
    read_inner(xml, maximum).map_err(|error| error.with_part(part))
}
fn read_inner<B: BufRead>(xml: &mut XmlStream<B>, maximum: usize) -> Result<PrintSettings> {
    let mut settings = PrintSettings::default();
    let mut properties = crabxl_core::SheetProperties::default();
    let mut seen = [false; 6];
    let mut leaf_depth = None;
    let mut break_container = None;
    let mut sheet_pr = false;
    let mut root = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"worksheet"
                {
                    return Err(invalid("Print source is not a worksheet"));
                }
                root = true;
            }
            Event::Start(_) if leaf_depth.is_some() => return Err(unsupported()),
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheetPr" =>
            {
                sheet_pr = true;
                properties.attributes = schema_attributes(&e, SHEET_PROPERTIES)?;
            }
            Event::End(e)
                if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheetPr" =>
            {
                sheet_pr = false
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && sheet_pr
                    && frame.depth == 3
                    && matches!(
                        e.local_name().as_ref().as_bytes(),
                        b"tabColor" | b"outlinePr"
                    ) =>
            {
                if e.local_name().as_ref().as_bytes() == b"tabColor" {
                    properties.tab_color = Some(schema_attributes(&e, TAB_COLOR)?);
                } else {
                    properties.outline = Some(schema_attributes(&e, OUTLINE)?);
                }
                leaf_depth = Some(frame.depth);
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheetFormatPr" =>
            {
                let format = schema_attributes(&e, SHEET_FORMAT)?;
                // The default format is the writers' output, so it stays unmodeled.
                let default = [("baseColWidth", "8"), ("defaultRowHeight", "15")];
                let is_default = format.len() == default.len()
                    && format
                        .iter()
                        .zip(default)
                        .all(|((name, value), (n, v))| name.as_ref() == n && value.as_ref() == v);
                settings.format = (!is_default).then_some(format);
                leaf_depth = Some(frame.depth);
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && ((frame.depth == 2
                        && matches!(
                            e.local_name().as_ref().as_bytes(),
                            b"printOptions"
                                | b"pageMargins"
                                | b"pageSetup"
                                | b"rowBreaks"
                                | b"colBreaks"
                        ))
                        || (sheet_pr
                            && frame.depth == 3
                            && e.local_name().as_ref().as_bytes() == b"pageSetUpPr")) =>
            {
                let name = e.local_name();
                let index = match name.as_ref().as_bytes() {
                    b"printOptions" => 0,
                    b"pageMargins" => 1,
                    b"pageSetup" => 2,
                    b"rowBreaks" => 3,
                    b"colBreaks" => 4,
                    _ => 5,
                };
                if seen[index] {
                    return Err(invalid("Duplicate printing metadata element"));
                }
                seen[index] = true;
                attributes(&e, |name, value| {
                    match index {
                        0 => match name {
                            b"horizontalCentered" => {
                                settings.options.horizontal_centered = Some(boolean(value)?)
                            }
                            b"verticalCentered" => {
                                settings.options.vertical_centered = Some(boolean(value)?)
                            }
                            b"headings" => settings.options.headings = Some(boolean(value)?),
                            b"gridLines" => settings.options.grid_lines = Some(boolean(value)?),
                            b"gridLinesSet" => {
                                settings.options.grid_lines_set = Some(boolean(value)?)
                            }
                            _ => return Err(unsupported()),
                        },
                        1 => {
                            let number: f64 =
                                value.parse().map_err(|_| invalid("Invalid page margin"))?;
                            let margins = settings
                                .margins
                                .as_mut()
                                .ok_or_else(|| invalid("Missing page margins"))?;
                            match name {
                                b"left" => margins.left = number,
                                b"right" => margins.right = number,
                                b"top" => margins.top = number,
                                b"bottom" => margins.bottom = number,
                                b"header" => margins.header = number,
                                b"footer" => margins.footer = number,
                                _ => return Err(unsupported()),
                            }
                        }
                        2 => {
                            let setup = &mut settings.setup;
                            match name {
                                b"paperSize" => setup.paper_size = Some(integer(value)?),
                                b"scale" => setup.scale = Some(integer(value)?),
                                b"fitToHeight" => setup.fit_to_height = Some(integer(value)?),
                                b"fitToWidth" => setup.fit_to_width = Some(integer(value)?),
                                b"firstPageNumber" => {
                                    setup.first_page_number = Some(integer(value)?)
                                }
                                b"horizontalDpi" => setup.horizontal_dpi = Some(integer(value)?),
                                b"verticalDpi" => setup.vertical_dpi = Some(integer(value)?),
                                b"copies" => setup.copies = Some(integer(value)?),
                                b"useFirstPageNumber" => {
                                    setup.use_first_page_number = Some(boolean(value)?)
                                }
                                b"usePrinterDefaults" => {
                                    setup.use_printer_defaults = Some(boolean(value)?)
                                }
                                b"blackAndWhite" => setup.black_and_white = Some(boolean(value)?),
                                b"draft" => setup.draft = Some(boolean(value)?),
                                b"orientation" if value == "none" => setup.orientation = None,
                                b"orientation" => {
                                    setup.orientation = Some(PageOrientation::parse(value)?)
                                }
                                b"pageOrder" if value == "none" => setup.page_order = None,
                                b"pageOrder" => setup.page_order = Some(PageOrder::parse(value)?),
                                b"cellComments" if value == "none" => setup.cell_comments = None,
                                b"cellComments" => {
                                    setup.cell_comments = Some(PrintedComments::parse(value)?)
                                }
                                b"errors" if value == "none" => setup.errors = None,
                                b"errors" => setup.errors = Some(PrintedErrors::parse(value)?),
                                b"paperHeight" => {
                                    setup.paper_height = Some(PaperDimension::parse(value)?)
                                }
                                b"paperWidth" => {
                                    setup.paper_width = Some(PaperDimension::parse(value)?)
                                }
                                name if name.ends_with(b":id") => {
                                    // Office relationship namespace identity is checked below.
                                    setup.printer_relationship = Some(value.into());
                                }
                                _ => return Err(unsupported()),
                            }
                        }
                        3 | 4 => match name {
                            b"count" | b"manualBreakCount" => {
                                integer(value)?;
                            }
                            _ => return Err(unsupported()),
                        },
                        _ => match name {
                            b"autoPageBreaks" => settings.auto_page_breaks = Some(boolean(value)?),
                            b"fitToPage" => settings.fit_to_page = Some(boolean(value)?),
                            _ => return Err(unsupported()),
                        },
                    }
                    Ok(())
                })?;
                if index == 2 {
                    let expected = frame.office_relationship.as_deref();
                    if settings.setup.printer_relationship.as_deref() != expected {
                        return Err(invalid(
                            "Printer identity is not an office relationship attribute",
                        ));
                    }
                }
                if index == 3 || index == 4 {
                    break_container = Some(index);
                } else {
                    leaf_depth = Some(frame.depth);
                }
            }
            Event::Start(e) if break_container.is_some() => {
                if frame.scope != Scope::Spreadsheet
                    || frame.depth != 3
                    || e.local_name().as_ref().as_bytes() != b"brk"
                {
                    return Err(unsupported());
                }
                let mut entry = PageBreak::default();
                attributes(&e, |name, value| {
                    match name {
                        b"id" => entry.id = Some(integer(value)?),
                        b"min" => entry.minimum = Some(integer(value)?),
                        b"max" => entry.maximum = Some(integer(value)?),
                        b"man" => entry.manual = Some(boolean(value)?),
                        b"pt" => entry.pivot = Some(boolean(value)?),
                        _ => return Err(unsupported()),
                    }
                    Ok(())
                })?;
                if settings
                    .memory_bytes()
                    .saturating_add(size_of::<PageBreak>())
                    > maximum
                {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Page break metadata allowance exceeded",
                    ));
                }
                let list = if break_container == Some(3) {
                    &mut settings.row_breaks
                } else {
                    &mut settings.column_breaks
                };
                list.try_reserve_exact(1).map_err(|error| {
                    Error::caused_by(
                        ErrorKind::MemoryBudgetExceeded,
                        "Cannot allocate page breaks",
                        error,
                    )
                })?;
                list.push(entry);
                leaf_depth = Some(3);
            }
            Event::End(_) if leaf_depth.is_some_and(|depth| frame.depth + 1 == depth) => {
                leaf_depth = None
            }
            Event::End(e)
                if frame.depth == 1
                    && matches!(
                        e.local_name().as_ref().as_bytes(),
                        b"rowBreaks" | b"colBreaks"
                    ) =>
            {
                break_container = None
            }
            Event::Text(e) if leaf_depth.is_some() || break_container.is_some() => {
                if !e.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) {
                    return Err(invalid("Unexpected printing metadata text"));
                }
            }
            Event::CData(_) | Event::GeneralRef(_)
                if leaf_depth.is_some() || break_container.is_some() =>
            {
                return Err(invalid("Unexpected printing metadata content"));
            }
            Event::Eof => {
                if !root {
                    return Err(invalid("Missing printing worksheet root"));
                }
                break;
            }
            _ => {}
        }
        if settings.memory_bytes() > maximum {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Printing metadata allowance exceeded",
            ));
        }
    }
    settings.properties = (!properties.is_empty()).then_some(properties);
    settings.validate()?;
    if settings.memory_bytes() > maximum {
        return Err(Error::new(
            ErrorKind::MemoryBudgetExceeded,
            "Printing metadata allowance exceeded",
        ));
    }
    Ok(settings)
}

/// Original-package replacement preserves unrelated nodes and schema tail ordering.
pub(crate) struct Rewrite<'a> {
    settings: Option<&'a PrintSettings>,
    /// `Some` replaces `<headerFooter>`; `Some(None)` removes it.
    header_footer: Option<Option<&'a crabxl_core::features::HeaderFooter>>,
    header_footer_written: bool,
    sheet_pr_seen: bool,
    in_sheet_pr: bool,
    properties_written: bool,
    format_written: bool,
    page_written: bool,
    rows_written: bool,
    columns_written: bool,
}
impl<'a> Rewrite<'a> {
    pub(crate) fn new(settings: &'a PrintSettings) -> Self {
        Self::with_header_footer(Some(settings), None)
    }
    /// Replace printing settings and/or the header/footer. `None` keeps the
    /// source elements of that kind.
    pub(crate) fn with_header_footer(
        settings: Option<&'a PrintSettings>,
        header_footer: Option<Option<&'a crabxl_core::features::HeaderFooter>>,
    ) -> Self {
        Self {
            settings,
            header_footer,
            header_footer_written: false,
            sheet_pr_seen: false,
            in_sheet_pr: false,
            properties_written: false,
            format_written: false,
            page_written: false,
            rows_written: false,
            columns_written: false,
        }
    }
    fn page(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.page_written {
            if let Some(settings) = self.settings {
                write_page(output, settings, uri)?;
            }
            self.page_written = true;
        }
        Ok(())
    }
    fn header_footer(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.header_footer_written {
            if let Some(Some(value)) = self.header_footer {
                crate::features::header_footer::write(output, value, uri)?;
            }
            self.header_footer_written = true;
        }
        Ok(())
    }
    fn rows(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.rows_written {
            if let Some(settings) = self.settings {
                write_breaks(output, "rowBreaks", &settings.row_breaks, uri)?;
            }
            self.rows_written = true;
        }
        Ok(())
    }
    fn columns(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.columns_written {
            if let Some(settings) = self.settings {
                write_breaks(output, "colBreaks", &settings.column_breaks, uri)?;
            }
            self.columns_written = true;
        }
        Ok(())
    }
    pub(crate) fn before_start(
        &mut self,
        output: &mut impl Write,
        name: &[u8],
        depth: usize,
        uri: Option<&str>,
    ) -> io::Result<bool> {
        let Some(settings) = self.settings else {
            return self.header_footer_only(output, name, depth, uri);
        };
        if depth == 2 && name == b"sheetPr" {
            if settings.properties.is_some() {
                // Modeled properties hold every sheetPr child, so the source element is replaced.
                write_properties(output, settings, uri)?;
                self.properties_written = true;
                return Ok(true);
            }
            self.sheet_pr_seen = true;
            self.in_sheet_pr = true;
            return Ok(false);
        }
        if depth == 2 && !self.sheet_pr_seen && !self.properties_written {
            write_properties(output, settings, uri)?;
            self.properties_written = true;
        }
        if depth == 3 && self.in_sheet_pr && name == b"pageSetUpPr" {
            write_page_properties(output, settings, uri)?;
            self.properties_written = true;
            return Ok(true);
        }
        if depth != 2 {
            return Ok(false);
        }
        let rank = match name {
            b"printOptions" => 1,
            b"pageMargins" => 2,
            b"pageSetup" => 3,
            b"headerFooter" => 4,
            b"rowBreaks" => 5,
            b"colBreaks" => 6,
            b"customProperties" | b"cellWatches" | b"ignoredErrors" | b"smartTags" | b"drawing"
            | b"legacyDrawing" | b"legacyDrawingHF" | b"picture" | b"oleObjects" | b"controls"
            | b"webPublishItems" | b"tableParts" | b"extLst" => 7,
            _ => 0,
        };
        if rank >= 1 {
            self.page(output, uri)?;
        }
        if rank == 4 && self.header_footer.is_some() {
            self.header_footer(output, uri)?;
            return Ok(true);
        }
        if rank >= 5 {
            self.header_footer(output, uri)?;
        }
        if rank >= 5 {
            self.rows(output, uri)?;
        }
        if rank >= 6 {
            self.columns(output, uri)?;
        }
        Ok(matches!(
            name,
            b"printOptions" | b"pageMargins" | b"pageSetup" | b"rowBreaks" | b"colBreaks"
        ))
    }
    /// Replace or insert `<sheetFormatPr>`. Callers run this after worksheet views
    /// are written, so the schema order sheetViews, sheetFormatPr, cols holds.
    pub(crate) fn format(
        &mut self,
        output: &mut impl Write,
        name: &[u8],
        depth: usize,
        uri: Option<&str>,
    ) -> io::Result<bool> {
        let Some(settings) = self.settings else {
            return Ok(false);
        };
        if depth != 2
            || self.format_written
            || !matches!(name, b"sheetFormatPr" | b"cols" | b"sheetData")
        {
            return Ok(false);
        }
        self.format_written = true;
        if settings.format.is_none() {
            return Ok(false);
        }
        write_format(output, Some(settings), uri)?;
        Ok(name == b"sheetFormatPr")
    }
    fn header_footer_only(
        &mut self,
        output: &mut impl Write,
        name: &[u8],
        depth: usize,
        uri: Option<&str>,
    ) -> io::Result<bool> {
        if depth != 2 || self.header_footer.is_none() {
            return Ok(false);
        }
        if name == b"headerFooter" {
            self.header_footer(output, uri)?;
            return Ok(true);
        }
        let later = crate::features::worksheet_rank(name).is_some_and(|rank| {
            rank > crate::features::worksheet_rank(b"headerFooter").unwrap_or(0)
        });
        if later {
            self.header_footer(output, uri)?;
        }
        Ok(false)
    }
    pub(crate) fn before_end(
        &mut self,
        output: &mut impl Write,
        name: &[u8],
        depth: usize,
        uri: Option<&str>,
    ) -> io::Result<()> {
        if depth == 1 && name == b"sheetPr" {
            if !self.properties_written
                && let Some(settings) = self.settings
            {
                write_page_properties(output, settings, uri)?;
                self.properties_written = true;
            }
            self.in_sheet_pr = false;
        }
        if depth == 0 && name == b"worksheet" {
            self.page(output, uri)?;
            self.header_footer(output, uri)?;
            self.rows(output, uri)?;
            self.columns(output, uri)?;
        }
        Ok(())
    }
}
