//! Checked source guards and borrowed row serialization for canonical model edits.
//! This coordinator is original CrabXL code, not an imported workbook engine.
use crate::encode::{RowBuffer, StyleContext, ValueEncoding, encode_cells_with_dimension};
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
pub(crate) fn guard<B: BufRead>(xml: &mut XmlStream<B>) -> Result<bool> {
    let mut shared_strings = false;
    let mut data = false;
    let mut seen_data = false;
    let mut columns = false;
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
                }
            }
            Event::End(e) if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"cols" => {
                columns = false;
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
pub(crate) fn guard_strings<B: BufRead>(xml: &mut XmlStream<B>) -> Result<()> {
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
    let mut cells = sheet.cells();
    let Some(cell) = cells.next() else {
        return "A1:A1".into();
    };
    let mut first = cell.address;
    let mut last = cell.address;
    for cell in cells {
        first.row = first.row.min(cell.address.row);
        first.column = first.column.min(cell.address.column);
        last.row = last.row.max(cell.address.row);
        last.column = last.column.max(cell.address.column);
    }
    format!("{first}:{last}")
}

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
    write!(output, "<sheetData xmlns=\"{uri}\">")
        .map_err(|cause| crate::writer::io_error("Cannot start model sheetData", cause))?;
    while cells.peek().is_some() || dimensions.peek().is_some() {
        let row = match (cells.peek(), dimensions.peek()) {
            (Some(cell), Some(dimension)) => (*cell).min(*dimension),
            (Some(cell), None) => *cell,
            (None, Some(dimension)) => *dimension,
            (None, None) => break,
        };
        if cells.peek() == Some(&row) {
            cells.next();
        }
        if dimensions.peek() == Some(&row) {
            dimensions.next();
        }
        encode_cells_with_dimension(
            &mut buffer,
            (row, sheet.dimensions().row(row)),
            sheet.row_cells(row),
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
        )?;
        output
            .write_all(&buffer.data)
            .map_err(|cause| crate::writer::io_error("Cannot write model row", cause))?;
        last = Some(row.get());
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
    crate::dimension_codec::write_columns(output, sheet.dimensions().columns())
        .map_err(|cause| crate::writer::io_error("Cannot write model columns", cause))?;
    write_data(output, sheet, catalog, limits, encoding, uri)?;
    output
        .write_all(b"</worksheet>")
        .map_err(|cause| crate::writer::io_error("Cannot finish new model worksheet", cause))
}
