//! Checked source guards and borrowed row serialization for canonical model edits.
//! This coordinator is original CrabXL code, not an imported workbook engine.
use crate::encode::{
    CellView, RowBuffer, StyleContext, ValueEncoding, encode_cell_views_with_dimension,
};
use crate::xml::{Scope, XmlStream};
use crabxl_core::{
    DateEpoch, Error, ErrorKind, ResourceLimits, Result, StyleCatalog, StyleId, TemporalStyleIds,
    Worksheet,
};
use quick_xml::events::Event;
use std::io::{BufRead, Write};

fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "Structural editing of affected worksheet feature graphs remains unimplemented",
    )
}
/// Validate the entire original worksheet before mutating its canonical cells.
/// Unaffected package parts remain on the source; affected unmodeled graphs must
/// not be silently erased when sheetData is serialized from the canonical bank.
pub(crate) fn guard<B: BufRead>(
    xml: &mut XmlStream<B>,
    rich_text: bool,
    maximum: usize,
) -> Result<bool> {
    let mut shared_strings = false;
    let mut data = false;
    let mut seen_data = false;
    let mut columns = false;
    let mut merges = false;
    loop {
        let frame = xml.next()?;
        match &frame.event {
            Event::Start(e) => {
                let name = e.local_name();
                let name = name.as_ref().as_bytes();
                if name == b"AlternateContent" {
                    return Err(unsupported());
                }
                if frame.depth == 1 {
                    if frame.scope != Scope::Spreadsheet || name != b"worksheet" {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "Invalid structural source root",
                        ));
                    }
                } else if frame.depth == 2 {
                    if frame.scope != Scope::Spreadsheet
                        || !matches!(
                            name,
                            b"sheetPr"
                                | b"dimension"
                                | b"sheetViews"
                                | b"sheetFormatPr"
                                | b"cols"
                                | b"sheetData"
                                | b"mergeCells"
                                | b"printOptions"
                                | b"pageMargins"
                                | b"pageSetup"
                                | b"headerFooter"
                        )
                    {
                        return Err(unsupported());
                    }
                    if name == b"sheetData" {
                        if seen_data {
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                "Duplicate structural sheetData",
                            ));
                        }
                        seen_data = true;
                        data = true;
                    }
                }
                if columns
                    && !(frame.depth == 3 && frame.scope == Scope::Spreadsheet && name == b"col")
                {
                    return Err(unsupported());
                }
                if frame.depth == 2 && name == b"cols" {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| unsupported())?;
                        let key = attr.key.as_ref().as_bytes();
                        if key != b"xmlns" && !key.starts_with(b"xmlns:") {
                            return Err(unsupported());
                        }
                    }
                    columns = true;
                }
                if frame.depth == 3 && name == b"col" {
                    if frame.scope != Scope::Spreadsheet {
                        return Err(unsupported());
                    }
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| unsupported())?;
                        let key = attr.key.as_ref().as_bytes();
                        if !matches!(
                            key,
                            b"min"
                                | b"max"
                                | b"width"
                                | b"style"
                                | b"hidden"
                                | b"bestFit"
                                | b"outlineLevel"
                                | b"collapsed"
                                | b"customWidth"
                                | b"phonetic"
                                | b"xmlns"
                        ) && !key.starts_with(b"xmlns:")
                        {
                            return Err(unsupported());
                        }
                    }
                }
                if frame.depth == 2 && name == b"mergeCells" {
                    merges = true;
                }
                if merges {
                    if frame.scope != Scope::Spreadsheet {
                        return Err(unsupported());
                    }
                    let allowed: &[&[u8]] = match (frame.depth, name) {
                        (2, b"mergeCells") => &[b"count"],
                        (3, b"mergeCell") => &[b"ref"],
                        _ => return Err(unsupported()),
                    };
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| unsupported())?;
                        let key = attr.key.as_ref().as_bytes();
                        if key != b"xmlns" && !key.starts_with(b"xmlns:") && !allowed.contains(&key)
                        {
                            return Err(unsupported());
                        }
                    }
                    if name == b"mergeCell" {
                        crate::xml::attribute(e, b"ref")?
                            .ok_or_else(|| {
                                Error::new(ErrorKind::InvalidData, "Merged range has no reference")
                            })?
                            .parse::<crabxl_core::CellRange>()?;
                    }
                }
                if data {
                    if name == b"c" && crate::xml::attribute(e, b"t")?.as_deref() == Some("s") {
                        shared_strings = true;
                    }
                    if frame.scope != Scope::Spreadsheet {
                        return Err(unsupported());
                    }
                    let allowed: &[&[u8]] = match (frame.depth, name) {
                        (2, b"sheetData") => &[],
                        (3, b"row") => &[
                            b"r",
                            b"spans",
                            b"ht",
                            b"s",
                            b"hidden",
                            b"outlineLevel",
                            b"collapsed",
                            b"customHeight",
                            b"customFormat",
                            b"thickTop",
                            b"thickBot",
                        ],
                        (4, b"c") => &[b"r", b"s", b"t"],
                        (5, b"f") => &[],
                        (5, b"v" | b"is") | (6, b"t") => &[],
                        _ => return Err(unsupported()),
                    };
                    for attr in e.attributes() {
                        let attr = attr.map_err(|cause| {
                            Error::caused_by(
                                ErrorKind::Xml,
                                "Invalid structural source attribute",
                                cause,
                            )
                        })?;
                        let key = attr.key.as_ref().as_bytes();
                        if key != b"xmlns"
                            && !key.starts_with(b"xmlns:")
                            && !(name == b"t" && key == b"xml:space")
                            && !allowed.contains(&key)
                        {
                            return Err(unsupported());
                        }
                    }
                    if rich_text && frame.depth == 5 && name == b"is" {
                        crate::rich_text::read_container(xml, 5, b"is", maximum, true)?;
                    }
                }
            }
            Event::End(e) if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"cols" => {
                columns = false;
            }
            Event::End(e)
                if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"mergeCells" =>
            {
                merges = false;
            }
            Event::Text(text)
                if merges && !text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
            {
                return Err(unsupported());
            }
            Event::CData(_) | Event::GeneralRef(_) | Event::Comment(_) | Event::PI(_) if merges => {
                return Err(unsupported());
            }
            Event::Text(text)
                if columns && !text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
            {
                return Err(unsupported());
            }
            Event::CData(_) | Event::GeneralRef(_) | Event::Comment(_) | Event::PI(_)
                if columns =>
            {
                return Err(unsupported());
            }
            Event::End(e)
                if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheetData" =>
            {
                data = false;
            }
            Event::Text(text)
                if data
                    && frame.depth <= 3
                    && !text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
            {
                return Err(unsupported());
            }
            Event::CData(_) | Event::GeneralRef(_) if data && frame.depth <= 3 => {
                return Err(unsupported());
            }
            Event::Comment(_) | Event::PI(_) if data => return Err(unsupported()),
            Event::DocType(_) => return Err(unsupported()),
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen_data {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Structural source has no sheetData",
        ));
    }
    Ok(shared_strings)
}
/// Plain shared strings can be re-encoded inline; rich/phonetic payloads must
/// not be flattened by a model loaded with the caller's plain-text projection.
pub(crate) fn guard_strings<B: BufRead>(
    xml: &mut XmlStream<B>,
    rich_text: bool,
    maximum: usize,
) -> Result<()> {
    loop {
        let frame = xml.next()?;
        match &frame.event {
            Event::Start(e) if frame.scope == Scope::Spreadsheet => {
                let name = e.local_name();
                if !matches!(
                    (frame.depth, name.as_ref().as_bytes()),
                    (1, b"sst") | (2, b"si") | (3, b"t")
                ) {
                    return Err(unsupported());
                }
                if frame.depth > 1 {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|cause| {
                            Error::caused_by(
                                ErrorKind::Xml,
                                "Invalid structural SST attribute",
                                cause,
                            )
                        })?;
                        let key = attr.key.as_ref().as_bytes();
                        if key != b"xmlns"
                            && !key.starts_with(b"xmlns:")
                            && !(frame.depth == 3 && key == b"xml:space")
                        {
                            return Err(unsupported());
                        }
                    }
                }
                if rich_text && frame.depth == 2 && name.as_ref().as_bytes() == b"si" {
                    crate::rich_text::read_container(xml, 2, b"si", maximum, true)?;
                }
            }
            Event::Comment(_) | Event::PI(_) if frame.depth >= 2 => return Err(unsupported()),
            Event::Text(text)
                if frame.depth < 3
                    && !text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
            {
                return Err(unsupported());
            }
            Event::CData(_) | Event::GeneralRef(_) if frame.depth < 3 => return Err(unsupported()),
            Event::Start(_) | Event::DocType(_) => return Err(unsupported()),
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}
pub(crate) fn validate_model(sheet: &Worksheet, catalog: Option<&StyleCatalog>) -> Result<()> {
    sheet.validate_style_links(catalog)?;
    sheet.dimensions().validate()?;
    for style in sheet
        .dimensions()
        .rows()
        .iter()
        .filter_map(|row| row.style)
        .chain(
            sheet
                .dimensions()
                .columns()
                .iter()
                .filter_map(|column| column.style),
        )
    {
        if catalog.map_or(style.get() != 0, |catalog| {
            catalog.cell_format(style).is_none()
        }) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Unknown dimension style identity",
            ));
        }
    }
    if sheet
        .dimensions()
        .rows()
        .iter()
        .any(|row| row.descent.is_some())
    {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "Extended row descent serialization is not implemented",
        ));
    }
    let mut styles = StyleContext::Catalog(catalog);
    for cell in sheet.cells() {
        if matches!(&cell.value, crabxl_core::CellValue::Formula(formula)
            if formula.formula_type() != crabxl_core::FormulaType::Normal)
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Structural formula-group editing remains unimplemented",
            )
            .with_cell(cell.address));
        }
        if let Some(date) = cell.value.temporal_value() {
            styles
                .prepare_date_format(cell.style.get(), date)
                .map_err(|error| error.with_cell(cell.address))?;
        }
    }
    Ok(())
}
pub(crate) fn dimension(sheet: &Worksheet) -> String {
    let bounds = sheet
        .cells()
        .map(|cell| (cell.address, cell.address))
        .chain(
            sheet
                .merged_ranges()
                .ranges()
                .iter()
                .map(|range| (range.range().start, range.range().end)),
        );
    let mut first: Option<crabxl_core::CellAddress> = None;
    let mut last: Option<crabxl_core::CellAddress> = None;
    for (start, end) in bounds {
        first = Some(first.map_or(start, |first| crabxl_core::CellAddress {
            row: first.row.min(start.row),
            column: first.column.min(start.column),
        }));
        last = Some(last.map_or(end, |last| crabxl_core::CellAddress {
            row: last.row.max(end.row),
            column: last.column.max(end.column),
        }));
    }
    match (first, last) {
        (Some(first), Some(last)) => format!("{first}:{last}"),
        _ => "A1:A1".into(),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Encoding {
    pub(crate) epoch: DateEpoch,
    pub(crate) non_finite: crate::NonFiniteWritePolicy,
    pub(crate) formula_attributes: crate::FormulaWritePolicy,
}
/// Reuse the same checked encoder as sequential and owned-model writers. Only
/// one byte-bounded row buffer is resident; cell/style payloads stay borrowed.
pub(crate) fn write_data<W: Write>(
    output: &mut W,
    sheet: &Worksheet,
    catalog: Option<&StyleCatalog>,
    limits: ResourceLimits,
    encoding: Encoding,
    uri: &str,
) -> Result<()> {
    let mut buffer = RowBuffer {
        data: Vec::new(),
        maximum: limits.max_row_bytes,
    };
    let mut cells = sheet.row_indices().peekable();
    let mut dimensions = sheet
        .dimensions()
        .rows()
        .iter()
        .map(|row| row.index)
        .peekable();
    let mut last = None;
    let mut merged = crate::writer::next_merged_row(sheet, 0);
    write!(output, "<sheetData xmlns=\"{uri}\">")
        .map_err(|cause| crate::writer::io_error("Cannot start model sheetData", cause))?;
    while cells.peek().is_some() || dimensions.peek().is_some() || merged.is_some() {
        let Some(row) = cells
            .peek()
            .copied()
            .into_iter()
            .chain(dimensions.peek().copied())
            .chain(merged)
            .min()
        else {
            break;
        };
        if cells.peek() == Some(&row) {
            cells.next();
        }
        if dimensions.peek() == Some(&row) {
            dimensions.next();
        }
        if !sheet.merged_ranges().has_virtual_styles() {
            encode_model_row(
                &mut buffer,
                sheet,
                row,
                sheet.row_cells(row).map(CellView::from),
                catalog,
                limits,
                encoding,
            )?;
        } else {
            encode_model_row(
                &mut buffer,
                sheet,
                row,
                crate::writer::MergedRowCells::new(sheet, row),
                catalog,
                limits,
                encoding,
            )?;
        }
        output
            .write_all(&buffer.data)
            .map_err(|cause| crate::writer::io_error("Cannot write model row", cause))?;
        last = Some(row.get());
        merged = crate::writer::next_merged_row(sheet, row.get() + 1);
    }
    if sheet.row_extent() > 0 && last.is_none_or(|row| row + 1 < sheet.row_extent()) {
        write!(output, "<row r=\"{}\"/>", sheet.row_extent())
            .map_err(|cause| crate::writer::io_error("Cannot preserve model row extent", cause))?;
    }
    output
        .write_all(b"</sheetData>")
        .map_err(|cause| crate::writer::io_error("Cannot finish model sheetData", cause))
}

/// Minimal new worksheet envelope around the canonical shared row serializer.
pub(crate) fn write_new<W: Write>(
    output: &mut W,
    sheet: &Worksheet,
    catalog: Option<&StyleCatalog>,
    limits: ResourceLimits,
    encoding: Encoding,
    uri: &str,
) -> Result<()> {
    write!(output, "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"{uri}\"><dimension ref=\"{}\"/><sheetFormatPr defaultRowHeight=\"15\"/>", dimension(sheet))
        .map_err(|cause| crate::writer::io_error("Cannot start new model worksheet", cause))?;
    crate::dimension_codec::write_columns(output, sheet.dimensions().columns(), None)
        .map_err(|cause| crate::writer::io_error("Cannot write model columns", cause))?;
    write_data(output, sheet, catalog, limits, encoding, uri)?;
    write_merges(output, sheet, Some(uri))?;
    output
        .write_all(b"</worksheet>")
        .map_err(|cause| crate::writer::io_error("Cannot finish new model worksheet", cause))
}

fn encode_model_row<'a>(
    buffer: &mut RowBuffer,
    sheet: &Worksheet,
    row: crabxl_core::RowIndex,
    cells: impl Iterator<Item = CellView<'a>> + Clone,
    catalog: Option<&StyleCatalog>,
    limits: ResourceLimits,
    encoding: Encoding,
) -> Result<()> {
    encode_cell_views_with_dimension(
        buffer,
        (row, sheet.dimensions().row(row)),
        cells,
        limits.max_cell_bytes,
        limits.max_row_cells,
        StyleContext::Catalog(catalog),
        ValueEncoding {
            epoch: encoding.epoch,
            iso_dates: false,
            non_finite: encoding.non_finite,
            formula_attributes: encoding.formula_attributes,
            date_styles: TemporalStyleIds {
                datetime: StyleId::new(0),
                time: StyleId::new(0),
                duration: StyleId::new(0),
                date: StyleId::new(0),
            },
            invalidate_caches: true,
        },
    )
}

pub(crate) fn write_merges<W: Write>(
    output: &mut W,
    sheet: &Worksheet,
    uri: Option<&str>,
) -> Result<()> {
    let ranges = sheet.merged_ranges().ranges();
    if ranges.is_empty() {
        return Ok(());
    }
    (|| -> std::io::Result<()> {
        write!(output, "<mergeCells count=\"{}\"", ranges.len())?;
        if let Some(uri) = uri {
            write!(output, " xmlns=\"{uri}\"")?;
        }
        output.write_all(b">")?;
        for range in ranges {
            write!(output, "<mergeCell ref=\"{}\"/>", range.range())?;
        }
        output.write_all(b"</mergeCells>")
    })()
    .map_err(|cause| crate::writer::io_error("Cannot write merged geometry", cause))
}
