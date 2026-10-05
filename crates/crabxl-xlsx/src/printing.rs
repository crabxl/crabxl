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

pub(crate) fn validate(settings: &PrintSettings) -> Result<()> {
    settings.validate()?;
    validate_setup(&settings.setup)
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
fn namespace(output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
    if let Some(uri) = uri {
        write_attribute(output, "xmlns", uri)?;
    }
    Ok(())
}
pub(crate) fn write_properties(
    output: &mut impl Write,
    settings: &PrintSettings,
    uri: Option<&str>,
    wrap: bool,
) -> io::Result<()> {
    if settings.auto_page_breaks.is_none() && settings.fit_to_page.is_none() {
        return Ok(());
    }
    if wrap {
        output.write_all(b"<sheetPr")?;
        namespace(output, uri)?;
        output.write_all(b">")?;
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
    output.write_all(b"/>")?;
    if wrap {
        output.write_all(b"</sheetPr>")?;
    }
    Ok(())
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
                sheet_pr = true
            }
            Event::End(e)
                if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheetPr" =>
            {
                sheet_pr = false
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
    settings: &'a PrintSettings,
    sheet_pr_seen: bool,
    in_sheet_pr: bool,
    properties_written: bool,
    page_written: bool,
    rows_written: bool,
    columns_written: bool,
}
impl<'a> Rewrite<'a> {
    pub(crate) fn new(settings: &'a PrintSettings) -> Self {
        Self {
            settings,
            sheet_pr_seen: false,
            in_sheet_pr: false,
            properties_written: false,
            page_written: false,
            rows_written: false,
            columns_written: false,
        }
    }
    fn page(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.page_written {
            write_page(output, self.settings, uri)?;
            self.page_written = true;
        }
        Ok(())
    }
    fn rows(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.rows_written {
            write_breaks(output, "rowBreaks", &self.settings.row_breaks, uri)?;
            self.rows_written = true;
        }
        Ok(())
    }
    fn columns(&mut self, output: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        if !self.columns_written {
            write_breaks(output, "colBreaks", &self.settings.column_breaks, uri)?;
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
        if depth == 2 && name == b"sheetPr" {
            self.sheet_pr_seen = true;
            self.in_sheet_pr = true;
            return Ok(false);
        }
        if depth == 2 && !self.sheet_pr_seen && !self.properties_written {
            write_properties(output, self.settings, uri, true)?;
            self.properties_written = true;
        }
        if depth == 3 && self.in_sheet_pr && name == b"pageSetUpPr" {
            write_properties(output, self.settings, uri, false)?;
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
    pub(crate) fn before_end(
        &mut self,
        output: &mut impl Write,
        name: &[u8],
        depth: usize,
        uri: Option<&str>,
    ) -> io::Result<()> {
        if depth == 1 && name == b"sheetPr" {
            if !self.properties_written {
                write_properties(output, self.settings, uri, false)?;
                self.properties_written = true;
            }
            self.in_sheet_pr = false;
        }
        if depth == 0 && name == b"worksheet" {
            self.page(output, uri)?;
            self.rows(output, uri)?;
            self.columns(output, uri)?;
        }
        Ok(())
    }
}
