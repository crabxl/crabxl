// SPDX-License-Identifier: MIT
// Worksheet view/pane/selection layouts adapted from rust_xlsxwriter worksheet.rs.
// Copyright 2022-2026 John McNamara. See third_party/ports.json.
use crate::encode::{validate_xml_text, write_attribute};
use crabxl_core::{Result, SheetViews};
use std::io::{self, Write};

pub(crate) fn validate(views: &SheetViews) -> Result<()> {
    views.validate()?;
    for view in &views.views {
        for value in view
            .top_left_cell
            .iter()
            .chain(view.pane.iter().flat_map(|p| p.top_left_cell.iter()))
        {
            validate_xml_text(value)?;
        }
        for selection in &view.selections {
            for value in selection.active_cell.iter().chain(selection.ranges.iter()) {
                validate_xml_text(value)?;
            }
        }
    }
    Ok(())
}

/// Borrow model payloads; numeric attributes use Write formatting without owned copies.
pub(crate) fn write_views(
    output: &mut impl Write,
    views: &SheetViews,
    namespace: Option<&str>,
) -> io::Result<()> {
    output.write_all(b"<sheetViews")?;
    if let Some(uri) = namespace {
        write_attribute(output, "xmlns", uri)?;
    }
    output.write_all(b">")?;
    for view in &views.views {
        output.write_all(b"<sheetView")?;
        for (name, value) in [
            ("windowProtection", view.window_protection),
            ("showFormulas", view.show_formulas),
            ("showGridLines", view.show_grid_lines),
            ("showRowColHeaders", view.show_row_column_headers),
            ("showZeros", view.show_zeros),
            ("rightToLeft", view.right_to_left),
            ("tabSelected", view.tab_selected),
            ("showRuler", view.show_ruler),
            ("showOutlineSymbols", view.show_outline_symbols),
            ("defaultGridColor", view.default_grid_color),
            ("showWhiteSpace", view.show_white_space),
            ("zoomToFit", view.zoom_to_fit),
        ] {
            if let Some(value) = value {
                write_attribute(output, name, if value { "1" } else { "0" })?;
            }
        }
        if let Some(mode) = view.mode {
            write_attribute(output, "view", mode.as_str())?;
        }
        if let Some(cell) = &view.top_left_cell {
            write_attribute(output, "topLeftCell", cell)?;
        }
        for (name, value) in [
            ("colorId", view.color_id),
            ("zoomScale", view.zoom_scale),
            ("zoomScaleNormal", view.zoom_scale_normal),
            ("zoomScaleSheetLayoutView", view.zoom_scale_sheet_layout),
            ("zoomScalePageLayoutView", view.zoom_scale_page_layout),
        ] {
            if let Some(value) = value {
                write!(output, " {name}=\"{value}\"")?;
            }
        }
        write!(output, " workbookViewId=\"{}\">", view.workbook_view_id)?;
        if let Some(pane) = &view.pane {
            output.write_all(b"<pane")?;
            if let Some(value) = pane.x_split {
                write!(output, " xSplit=\"{value}\"")?;
            }
            if let Some(value) = pane.y_split {
                write!(output, " ySplit=\"{value}\"")?;
            }
            if let Some(value) = &pane.top_left_cell {
                write_attribute(output, "topLeftCell", value)?;
            }
            write_attribute(output, "activePane", pane.active_pane.as_str())?;
            write_attribute(output, "state", pane.state.as_str())?;
            output.write_all(b"/>")?;
        }
        for selection in &view.selections {
            output.write_all(b"<selection")?;
            if let Some(pane) = selection.pane {
                write_attribute(output, "pane", pane.as_str())?;
            }
            if let Some(cell) = &selection.active_cell {
                write_attribute(output, "activeCell", cell)?;
            }
            if let Some(id) = selection.active_cell_id {
                write!(output, " activeCellId=\"{id}\"")?;
            }
            if let Some(ranges) = &selection.ranges {
                write_attribute(output, "sqref", ranges)?;
            }
            output.write_all(b"/>")?;
        }
        output.write_all(b"</sheetView>")?;
    }
    output.write_all(b"</sheetViews>")
}

use crate::metadata::{attributes, boolean, integer};
use crate::xml::{Scope, XmlStream};
use crabxl_core::{
    Error, ErrorKind, Pane, PanePosition, PaneState, Selection, SheetView, ViewMode,
};
use quick_xml::events::Event;
use std::io::BufRead;

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn position(value: &str) -> Result<PanePosition> {
    match value {
        "topLeft" => Ok(PanePosition::TopLeft),
        "topRight" => Ok(PanePosition::TopRight),
        "bottomLeft" => Ok(PanePosition::BottomLeft),
        "bottomRight" => Ok(PanePosition::BottomRight),
        _ => Err(invalid("Invalid worksheet pane position")),
    }
}
fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "Worksheet view extension requires typed support; original package preservation remains available",
    )
}

// Header scanning deliberately stops before sheetData and does not validate the
// unconsumed worksheet payload/CRC. Retained views are independently byte bounded.
pub(crate) fn read_header<B: BufRead>(
    xml: &mut XmlStream<B>,
    maximum: usize,
) -> Result<SheetViews> {
    let part = xml.part().to_owned();
    read_header_inner(xml, maximum).map_err(|error| error.with_part(part))
}
fn read_header_inner<B: BufRead>(xml: &mut XmlStream<B>, maximum: usize) -> Result<SheetViews> {
    let mut views = SheetViews { views: Vec::new() };
    let mut current: Option<SheetView> = None;
    let mut in_views = false;
    let mut selections_seen = false;
    let mut pane_seen = false;
    let mut root_seen = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"worksheet"
                {
                    return Err(invalid("View source is not a worksheet"));
                }
                root_seen = true;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheetData" =>
            {
                break;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheetViews" =>
            {
                if in_views {
                    return Err(invalid("Duplicate worksheet views container"));
                }
                attributes(&e, |_, _| Err(unsupported()))?;
                in_views = true;
            }
            Event::Start(e) if in_views => {
                if frame.scope != Scope::Spreadsheet {
                    return Err(unsupported());
                }
                match (frame.depth, e.local_name().as_ref().as_bytes()) {
                    (3, b"sheetView") => {
                        if current.is_some() {
                            return Err(invalid("Nested worksheet views"));
                        }
                        if maximum.saturating_sub(views.memory_bytes())
                            < size_of::<SheetView>() + size_of::<Selection>() + 4
                        {
                            return Err(Error::new(
                                ErrorKind::MemoryBudgetExceeded,
                                "Worksheet view metadata allowance exceeded",
                            ));
                        }
                        let mut view = SheetView::default();
                        attributes(&e, |name, value| {
                            match name {
                                b"windowProtection" => {
                                    view.window_protection = Some(boolean(value)?)
                                }
                                b"showFormulas" => view.show_formulas = Some(boolean(value)?),
                                b"showGridLines" => view.show_grid_lines = Some(boolean(value)?),
                                b"showRowColHeaders" => {
                                    view.show_row_column_headers = Some(boolean(value)?)
                                }
                                b"showZeros" => view.show_zeros = Some(boolean(value)?),
                                b"rightToLeft" => view.right_to_left = Some(boolean(value)?),
                                b"tabSelected" => view.tab_selected = Some(boolean(value)?),
                                b"showRuler" => view.show_ruler = Some(boolean(value)?),
                                b"showOutlineSymbols" => {
                                    view.show_outline_symbols = Some(boolean(value)?)
                                }
                                b"defaultGridColor" => {
                                    view.default_grid_color = Some(boolean(value)?)
                                }
                                b"showWhiteSpace" => view.show_white_space = Some(boolean(value)?),
                                b"zoomToFit" => view.zoom_to_fit = Some(boolean(value)?),
                                b"colorId" => view.color_id = Some(integer(value)?),
                                b"zoomScale" => view.zoom_scale = Some(integer(value)?),
                                b"zoomScaleNormal" => {
                                    view.zoom_scale_normal = Some(integer(value)?)
                                }
                                b"zoomScaleSheetLayoutView" => {
                                    view.zoom_scale_sheet_layout = Some(integer(value)?)
                                }
                                b"zoomScalePageLayoutView" => {
                                    view.zoom_scale_page_layout = Some(integer(value)?)
                                }
                                b"workbookViewId" => view.workbook_view_id = integer(value)?,
                                b"topLeftCell" => view.top_left_cell = Some(value.into()),
                                b"view" => {
                                    view.mode = Some(match value {
                                        "normal" => ViewMode::Normal,
                                        "pageLayout" => ViewMode::PageLayout,
                                        "pageBreakPreview" => ViewMode::PageBreakPreview,
                                        _ => return Err(invalid("Invalid worksheet view mode")),
                                    })
                                }
                                _ => return Err(unsupported()),
                            }
                            Ok(())
                        })?;
                        selections_seen = false;
                        pane_seen = false;
                        current = Some(view);
                    }
                    (4, b"pane") => {
                        let view = current
                            .as_mut()
                            .ok_or_else(|| invalid("Pane outside worksheet view"))?;
                        if pane_seen {
                            return Err(invalid("Duplicate worksheet pane"));
                        }
                        let mut pane = Pane::default();
                        attributes(&e, |name, value| {
                            match name {
                                b"xSplit" => {
                                    pane.x_split =
                                        Some(value.parse().map_err(|_| {
                                            invalid("Invalid horizontal pane split")
                                        })?)
                                }
                                b"ySplit" => {
                                    pane.y_split = Some(
                                        value
                                            .parse()
                                            .map_err(|_| invalid("Invalid vertical pane split"))?,
                                    )
                                }
                                b"topLeftCell" => pane.top_left_cell = Some(value.into()),
                                b"activePane" => pane.active_pane = position(value)?,
                                b"state" => {
                                    pane.state = match value {
                                        "split" => PaneState::Split,
                                        "frozen" => PaneState::Frozen,
                                        "frozenSplit" => PaneState::FrozenSplit,
                                        _ => return Err(invalid("Invalid worksheet pane state")),
                                    }
                                }
                                _ => return Err(unsupported()),
                            }
                            Ok(())
                        })?;
                        pane.validate()?;
                        view.pane = Some(Box::new(pane));
                        pane_seen = true;
                    }
                    (4, b"selection") => {
                        let view = current
                            .as_mut()
                            .ok_or_else(|| invalid("Selection outside worksheet view"))?;
                        if !selections_seen {
                            view.selections.clear();
                            selections_seen = true;
                        }
                        let mut selection = Selection::default();
                        attributes(&e, |name, value| {
                            match name {
                                b"pane" => selection.pane = Some(position(value)?),
                                b"activeCell" => selection.active_cell = Some(value.into()),
                                b"activeCellId" => selection.active_cell_id = Some(integer(value)?),
                                b"sqref" => selection.ranges = Some(value.into()),
                                _ => return Err(unsupported()),
                            }
                            Ok(())
                        })?;
                        // Charge actual capacity growth, not the vector's length.
                        if view.selections.len() == view.selections.capacity() {
                            let needed = views
                                .memory_bytes()
                                .saturating_add(size_of::<SheetView>())
                                .saturating_add(view.heap_bytes())
                                .saturating_add(size_of::<Selection>())
                                .saturating_add(selection.heap_bytes());
                            if needed > maximum {
                                return Err(Error::new(
                                    ErrorKind::MemoryBudgetExceeded,
                                    "Worksheet selections allowance exceeded",
                                ));
                            }
                            view.selections.try_reserve_exact(1).map_err(|error| {
                                Error::caused_by(
                                    ErrorKind::MemoryBudgetExceeded,
                                    "Cannot allocate worksheet selections",
                                    error,
                                )
                            })?;
                        }
                        view.selections.push(selection);
                    }
                    _ => return Err(unsupported()),
                }
            }
            Event::End(e)
                if in_views
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheetView" =>
            {
                let view = current
                    .take()
                    .ok_or_else(|| invalid("Missing worksheet view"))?;
                let needed = views
                    .memory_bytes()
                    .saturating_add(size_of::<SheetView>())
                    .saturating_add(view.heap_bytes());
                if needed > maximum {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Worksheet views allowance exceeded",
                    ));
                }
                views.views.try_reserve_exact(1).map_err(|error| {
                    Error::caused_by(
                        ErrorKind::MemoryBudgetExceeded,
                        "Cannot allocate worksheet views",
                        error,
                    )
                })?;
                views.views.push(view);
            }
            Event::End(e)
                if in_views
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"sheetViews" =>
            {
                break;
            }
            Event::Text(e) if in_views => {
                if !e.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) {
                    return Err(invalid("Unexpected text in worksheet views"));
                }
            }
            Event::CData(_) | Event::GeneralRef(_) if in_views => {
                return Err(invalid("Unexpected content in worksheet views"));
            }
            Event::Eof => {
                if !root_seen || in_views {
                    return Err(invalid("Incomplete worksheet view header"));
                }
                break;
            }
            _ => {}
        }
        let retained = views.memory_bytes().saturating_add(
            current
                .as_ref()
                .map_or(0, |view| size_of::<SheetView>() + view.heap_bytes()),
        );
        if retained > maximum {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Worksheet view metadata allowance exceeded",
            ));
        }
    }
    if views.views.is_empty() {
        views = SheetViews::default();
    }
    if views.memory_bytes() > maximum {
        return Err(Error::new(
            ErrorKind::MemoryBudgetExceeded,
            "Worksheet default view allowance exceeded",
        ));
    }
    Ok(views)
}
